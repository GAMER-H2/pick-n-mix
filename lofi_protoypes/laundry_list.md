Small Tasks:
- Atmospheres shaders need more refinement (human task, all agents ignore)
- Maybe fix the slight clipping of leaving atmospheres to play and having the track slightly clip in the first time that's enabled (Opus only, other agents ignore)
- Albumn art getting squished in full-screen queue view (should shrink like it did before the crossfade fade was added)
- Very good, some tweaks:
  - The dropdowns are cut off by the master mixer modal not extending the entire window. Have the drop downs allow to draw over the negative space as well and allow the effects dropdown to scroll in the first place
  - If the effects button is clicked and no audio block is selected, it still appears but says to select an audio block to start adding and editing effects in greyed out text right in the middle of the effects block

User feedback (from Max):
- Still to do: presets the user can save to copy a whole chain to other blocks

Medium Tasks:
- Duplicate song handling - (working but needs some refinement as there are a lot of true negatives)
- Favourite playlist, heart button for tracks, treated like a mix playlist
- Database and metadata overhaul (including Recognise seperate artists on the same track and don't have duplicates)
- Start radio in context menu (mix of genre matching, shuffle, and top picks)

Large Tasks:
- In the proper queue view, have shaders that create visuals live from the song in the background. These are any combinations of colours, patterns, shapes, moving effects (pulsing, bouncing, squish and squash), and even lyric detection (or pulled from music database if easier) to display words dynamically in the background popping up in random locations. It would be cool if this was 3D visuals but 2D would probably be more reasonable.
- Support for remote libraries (navidrome and Jellyfin) and merging/syncing with local libraries
- Support for remote syncing (probably WebDAV) that copies your songs from a remote source
- Mobile port (Android mainly, iOS if possible)
- Plugin support for other users to easily add features. Will need an API to access backend features and a way to add custom vue items into the interface. For example, a plugin that adds YT-DLP functionality that allows you to download songs and playlists from YouTube links. Or a plugin that adds a transcoding option in the context menu that uses ffmpeg. Or a plugin that adds a new effect into the DJ Mixer
