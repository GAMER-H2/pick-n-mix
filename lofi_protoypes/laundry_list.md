Small Tasks:
- Atmospheres shaders need more refinement (human task, all agents ignore)
- Atmopsheres need better looping points
- All atmospheres should be the volume of the rain track
- Skeleton screen for the playing next widget in full-screen queue view that shimmers and quickly fades in the queue when it's ready

Medium Tasks:
- Duplicate song handling - (working but needs some refinement as there are a lot of true negatives)
- Favourite playlist, heart button for tracks, treated like a mix playlist

Large Tasks:
- In the proper queue view, have shaders that create visuals live from the song in the background. These are any combinations of colours, patterns, shapes, moving effects (pulsing, bouncing, squish and squash), and even lyric detection (or pulled from music database if easier) to display words dynamically in the background popping up in random locations. It would be cool if this was 3D visuals but 2D would probably be more reasonable.
- Support for remote libraries (navidrome and Jellyfin) and merging/syncing with local libraries
- Support for remote syncing (probably WebDAV) that copies your songs from a remote source
- Mobile port (Android mainly, iOS if possible)
- Plugin support for other users to easily add features. Will need an API to access backend features and a way to add custom vue items into the interface. For example, a plugin that adds YT-DLP functionality that allows you to download songs and playlists from YouTube links. Or a plugin that adds a transcoding option in the context menu that uses ffmpeg. Or a plugin that adds a new effect into the DJ Mixer
