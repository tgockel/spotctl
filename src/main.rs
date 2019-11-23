extern crate failure;
extern crate rand;
extern crate rspotify;
#[macro_use]
extern crate structopt;

mod cmd;

use std::error::Error;
use std::collections::HashSet;
use std::iter::FromIterator;
use std::time::Duration;
use std::thread;

use rand::thread_rng;
use rand::seq::SliceRandom;
use rspotify::spotify::oauth2::{SpotifyOAuth, SpotifyClientCredentials};
use rspotify::spotify::util::get_token;
use rspotify::spotify::client::Spotify;
use rspotify::spotify::model::playlist::{SimplifiedPlaylist, PlaylistTrack};
use rspotify::spotify::model::page::Page;
use rspotify::spotify::client::ApiError;
use structopt::StructOpt;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct Client {
    native: Spotify,
    user_id: String,
}

impl Client {
    pub fn new() -> Result<Client> {
        let mut oauth = SpotifyOAuth::default()
            .scope("user-library-read playlist-read-private playlist-modify-private playlist-modify-public")
            .redirect_uri("http://localhost:8888/callback")
            .build();

        let native = match get_token(&mut oauth) {
            Some(token_info) => {
                let client_credentials = SpotifyClientCredentials::default()
                    .token_info(token_info)
                    .build();
                Spotify::default()
                    .client_credentials_manager(client_credentials)
                    .build()
            }
            None => {
                // TODO: Better error message here...rspotify should probably return an `Error`
                return Err(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                                        "Could not create Spotify client")))
            }
        };

        let user_id = native.current_user()?.id;

        Ok(Client { native, user_id })
    }

    fn call_api<F, T>(func: F) -> std::result::Result<T, failure::Error>
        where F: Fn() -> std::result::Result<T, failure::Error> {

        loop {
            match func() {
                Ok(x) => return Ok(x),
                Err(e) => {
                    if let Some(ApiError::RateLimited(timeout)) = e.downcast_ref() {
                        // TODO: better tracking of this
                        eprintln!("Rate limited to {:?}", timeout);
                        thread::sleep(Duration::from_secs(timeout.unwrap_or(1usize) as u64))
                    } else {
                        return Err(e)
                    }
                }
            }
        }
    }

    fn get_all<F, T>(get_page: F) -> Result<Vec<T>>
        where F: Fn(u32) -> std::result::Result<Page<T>, failure::Error> {
        let meta = Self::call_api(|| get_page(0))?;
        let mut out = Vec::with_capacity(meta.total as usize);

        let mut offset = 0u32;
        while offset < meta.total {
            let mut res = Self::call_api(|| get_page(offset))?;
            if res.items.is_empty() {
                // This isn't really a problem -- the user might have altered the playlist since the
                // initial request
                eprintln!("Got 0 items in request for offset={}", offset);
                break
            }

            offset += res.items.len() as u32;
            out.append(&mut res.items);
        }

        Ok(out)
    }

    pub fn current_user_playlists(&self) -> Result<Vec<SimplifiedPlaylist>> {
        Self::get_all(|off| self.native.current_user_playlists(None, off))
    }

    /// Create a playlist with the given `name` and return the playlist ID.
    pub fn create_playlist(&self, name: &str, description: Option<&str>) -> Result<String> {
        let description = description.unwrap_or("Automatically-generated shuffled playlist");

        Ok(Self::call_api(|| {
            self.native.user_playlist_create(self.user_id.as_str(),
                                             name,
                                             false,
                                             description.to_owned())
        })?.id)
    }

    pub fn playlist_tracks(&self, playlist: &SimplifiedPlaylist) -> Result<Vec<PlaylistTrack>> {
        Self::get_all(
            |off| {
                self.native.user_playlist_tracks(self.user_id.as_str(),
                                                 playlist.id.as_str(),
                                                 None,
                                                 None,
                                                 off,
                                                 None)
            })
    }

    pub fn set_playlist(&self, playlist_id: &str, track_ids: &[String]) -> Result<()> {
        // Clear the playlist
        Self::call_api(||
            self.native.user_playlist_replace_tracks(self.user_id.as_str(), playlist_id, &[])
        )?;

        for track_id_chunk in track_ids.chunks(100) {
            Self::call_api(||
                self.native.user_playlist_add_tracks(self.user_id.as_str(), playlist_id, track_id_chunk, None)
            )?;
        }
        Ok(())
    }
}

/// A group of tracks. This generally represents an album, but can be any grouped unit that one
/// would want to shuffle.
#[derive(Debug)]
struct TrackGroup {
    /// An arbitrary name to give this group (usually the album name).
    pub name: String,
    pub track_ids: Vec<String>,
    /// Total length of time of all tracks in this group. It is the responsibility of the creation
    /// function to ensure this is correct.
    pub duration: Duration,
}

impl From<&[PlaylistTrack]> for TrackGroup {
    fn from(src: &[PlaylistTrack]) -> Self {
        assert!(!src.is_empty());

        TrackGroup{
            name: src[0].track.album.name.to_owned(),
            track_ids: Vec::from_iter(src.iter().map(|t| t.track.id.as_ref().unwrap().to_owned())),
            duration: src
                .iter()
                .fold(Duration::new(0, 0),
                      |acc, x| acc + Duration::from_millis(x.track.duration_ms as u64))
        }
    }
}

fn partition_by_album(mut src_tracks: &[PlaylistTrack]) -> Vec<TrackGroup> {
    let mut out = Vec::new();

    while !src_tracks.is_empty() {
        assert!(src_tracks[0].track.album.id.is_some());
        let split_idx = src_tracks
            .iter()
            .position(|t| t.track.album.id != src_tracks[0].track.album.id)
            .unwrap_or(src_tracks.len());

        let (next_tracks, remaining) = src_tracks.split_at(split_idx);
        src_tracks = remaining;

        let group = TrackGroup::from(next_tracks);
        // If the group is less than 10 minutes, it isn't an album
        if group.duration > Duration::from_secs(600u64) {
            out.push(group);
        }
    }

    out
}

fn partition_groups(playlist_name: &str, src_tracks: &[PlaylistTrack]) -> Vec<TrackGroup> {
    let duration = src_tracks
        .iter()
        .fold(Duration::new(0, 0),
              |acc, x| acc + Duration::from_millis(x.track.duration_ms as u64));

    if duration > Duration::from_secs(60u64 * 45) && duration < Duration::from_secs(60u64 * 90) {
        let mut group = TrackGroup::from(src_tracks);
        group.name = playlist_name.to_string();
        vec![group]
    } else {
        partition_by_album(src_tracks)
    }
}

fn load_groups(client: &Client) -> Result<Vec<TrackGroup>> {
    let banned_playlist_names: HashSet<&str> =
        ["Discover Weekly", "Starred", "Liked from Radio", "Shuffle"].iter().cloned().collect();

    let mut groups = Vec::new();
    for playlist in client.current_user_playlists()? {
        if banned_playlist_names.contains(playlist.name.as_str()) {
            continue
        }

        let tracks = client.playlist_tracks(&playlist)?;
        let mut pl_groups = partition_groups(playlist.name.as_str(), tracks.as_slice());
        groups.append(&mut pl_groups);
    }

    Ok(groups)
}

/// Create a playlist from `src`.
///
/// Returns a list of track IDs.
fn create_playlist(mut src: Vec<TrackGroup>, goal_duration: Option<Duration>) -> Vec<String> {
    let goal_duration = goal_duration.unwrap_or(Duration::from_secs(60u64 * 1200));

    let mut rng = thread_rng();
    src.shuffle(&mut rng);

    let mut playlist_duration = Duration::new(0, 0);
    let mut out = Vec::new();
    for group in src.iter() {
        if playlist_duration > goal_duration {
            break
        }

        eprintln!(" + {}", group.name);
        out.reserve(group.track_ids.len());
        for id in group.track_ids.iter() {
            out.push(id.clone());
        }
        playlist_duration += group.duration;
    }

    eprintln!("Play time: {} hours", playlist_duration.as_secs_f64() / 3600.0);
    out
}

/// Get the playlist ID for the shuffle output. This will either create a new playlist named
/// `"Shuffle"` or pick the playlist with that name.
fn get_or_create_shuffle_playlist_id(client: &Client) -> Result<String> {
    let name = "Shuffle";

    for playlist in client.current_user_playlists()? {
        if playlist.name.as_str() == name {
            eprintln!("Reusing existing playlist with ID={}", playlist.id);
            return Ok(playlist.id)
        }
    }

    client.create_playlist(name, None)
}

fn shuffle_library() -> Result<()> {
    let client = Client::new()?;
    let groups = load_groups(&client)?;

    let track_ids = create_playlist(groups, None);
    let playlist_id = get_or_create_shuffle_playlist_id(&client)?;

    client.set_playlist(playlist_id.as_str(), track_ids.as_slice())
}

fn main() -> Result<()> {
    use cmd::BaseCmd::*;

    let opts = cmd::BaseOpts::from_args();
    match opts.command {
        ShuffleLibrary => shuffle_library()
    }
}
