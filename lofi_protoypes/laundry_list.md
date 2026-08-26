Small Tasks:
- Switching songs isn't instant (implemented, not human tested)
- Full-screen queue view slide in is quite choppy, needs to slide over the content currently on the page quickly (having the page still below it while on screen). If the animation isn't snappy (both quick and non-laggy), then it should be scraped
- Back and forward buttons should count the song, album, and artist tabs in the library page

Medium Tasks:
- Duplicate song handling - same song gets collapsed into one, combining metadata in the program's database, file that is played back is the one with the highest quality (bitrate, sample rate, file type)
- Settings page - Theme, fade pause/play, keep reverb on pause, library management, output device override select
- Expanded EQ controls brings up a dialogue modal pop-up that is a clone of Logic Pro's EQ controls in this application's style
- Crossfade global controls in DJ Mixer - simple slider for crossfade length in pop-up DJ Mixer, graph controls for global DJ Mixer that shows the keyframes of each song to manipulate
- Home page showing recently played, archive mix, most played playlists, and simple recommendations

Large Tasks:
- Playlists can have custom crossfades between each song. These allow you to start fading anywhere in either song, sample a section of a song (or even import an audio file) to place at certain places on a timeline during the crossfade (including looping), add select DJ Mixer effects to any sample or just a specific part of one of the songs, and finally allow playlists to be exported as an MP3, FLAC, or WAV file (bounce the mix) that retains all of this and other settings.
- In the proper queue view, have shaders that create visuals live from the song in the background. These are any combinations of colours, patterns, shapes, moving effects (pulsing, bouncing, squish and squash), and even lyric detection (or pulled from music database if easier) to display words dynamically in the background popping up in random locations. It would be cool if this was 3D visuals but 2D would probably be more reasonable.
- Support for remote libraries (navidrome and Jellyfin) and merging/syncing with local libraries 
- Mobile port (Android mainly, iOS if possible)
