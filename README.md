# spotctl

Simple command-line tool for controlling the Spotify Web API.
This primarily exists because the Spotify client does not shuffle tracks in the way that I would
like.

## Setting Up

Use of the command line tool requires you to register an application to authenticate with the
Spotify API.

1. Go to the [Spotify dashboard](https://developer.spotify.com/dashboard/applications)
2. Log In
3. Click *Create a Client ID* and create an app
4. Click *Edit Settings* on the newly-created app
5. Find the *Redirect URIs* section and add `http://localhost:8888/callback`
6. Copy the *Client ID* and `export CLIENT_ID={your client ID}` and the *Client Secret* to
   `export CLIENT_SECRET={your client secret}`
7. Run the program and your browser will pop up asking to log in to your Spotify account
8. After logging in, the page will be redirected to `http://localhost:8888/callback`, which will be
   unable to connect (this is expected) -- copy the URL to the command line running `spotctl`
   
Luckily this procedure will happen very infrequently.

## Tools

### Shuffle the User Library

> `spotctl shuffle-library`

Load track groups from every playlist in the user's library and generate a new, shuffled playlist
containing approximately 20 hours of music.

## Concepts

### Track Group

A "track group" is a logical grouping of tracks.
By default this is equivalent to an album.

Ordering of tracks is playlist-preserved, so if you choose to keep your playlists in album-order,
then they will be shuffled in album-order.
If your tracks are in a different order, then they will be in that order post-shuffle.
This is for cases where you might prefer a different track ordering than the original artist.
As an example, you might think the Lil Jon and the East Side Boyz masterpiece
[Kings of Crunk](https://open.spotify.com/playlist/0LxMpO3eNoerryXHxt0Iyx) should start with "BME
Click" and have most of the skits removed (you'd be right).
