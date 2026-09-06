Small Tasks:
- Atmospheres shaders need more refinement (human task, all agents ignore)
- Atmopsheres need better looping points
- All atmospheres should be the volume of the rain track
- Skeleton screen for the playing next widget in full-screen queue view that shimmers and quickly fades in the queue when it's ready
- Button on homepage to shuffle whole library
- Full context menu of playlist in sidebar
- Continue title and subtitle style starting in settings to library and both queues
- Settings in playback to set parameters for restarting and going previous track, how many seconds to move (sync keyboard shortcuts with buttons), atmospheres play without track playback
- Clikcing on drop down should close it
- Searching in playlists
- macOS security fix
- Fade in the art when crossover in enabled and have in time

Max Notes:
- Re-order the advanced DJ effects (not just visually but also the effects chain order (layering))
- BPM editing in master mixer
- Effects appear in bottom in master mixer as a layout option (look at Abelton for inspiration on this layout and chain order with its levels preview before next level)
- Customise what is in the mixer sidebar (general and master mix)
- ProQ3 inspiration: solo the effected area of the EQ (the band) to preview it (hold down the button)

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
