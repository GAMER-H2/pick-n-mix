Small Tasks:
- Support OS media controls (Linux KDE and macOS priority)
- Volume slider sticky points for each quarter (holding shift prevent this)
- Switching songs isn't instant
- Play Next and Add to Queue still not working (not just not being added to the queue visually on the queue view, are not actually there via testing play next and skipping a song does not play it)
- Full-screen queue view should slide in from the bottom
- DJ Mixer pop-up appears behind the playing next widget in the full-screen queue view
- Click anywhere else closes the DJ Mixer or Info pop-ups
- Clicking volume icon mutes the audio (new icon switched out for this which also for when slider is dragged to zero). Unmuting the audio put it back where it was before muting (if it was already at zero, it goes to 100%)
- Track info (artist and album) should be clickable in the bottom player
- Full-screen queue view and info box should allow information that won't fit on one line to add more line underneath to fit content. There should be a sensible size limit that, if reached, allows you to hover your mouse over for a view of the full text.
- Back and forward button should collapse the full-screen queue as if it was a page
- Shift clicking the DJ Mixer brings up the advanced view straight away
- Support for albums with different artists per song (seemingly route cause of duplicate albums)

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
