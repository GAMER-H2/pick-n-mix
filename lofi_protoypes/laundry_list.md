Small Tasks:
- Atmospheres shaders need more refinement (human task, all agents ignore)
- Maybe fix the slight clipping of leaving atmospheres to play and having the track slightly clip in the first time that's enabled

User feedback (from Max):
- Re-order the advanced DJ effects (not just visually but also the effects chain order (layering))
- Effects appear in bottom in master mixer as a layout option (look at Abelton for inspiration on this layout and chain order with its levels preview before next level) (perhaps we go full DAW style and have effects selected from a menu and ordered in a chain in the bottom row for each audio block, then have presets the user can save to copy the chain to other blocks)

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
