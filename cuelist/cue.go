package cuelist

import "time"

// I've borrowed heavily from: http://www.stagelightingprimer.com/index.html?slfs-control.html&2

// CueList stores a list of cues and can play them back
type Cue struct {
	// The name or label associated with the cue
	Name string

	// A cue's "time" is a measure of how long it takes the cue to complete, once it has been executed. Depending upon
	// the console, time(s), entered in minutes and seconds, can be entered for the cue as a whole or, individually,
	// for transitions in focus, intensity (up and/or down), and color, as well as for individual channels. Time (or
	// delay) applied to individual channels is called, "discrete" timing.
	FadeTime time.Time

	// The (optional) length of time (in seconds, after pressing the "Go" button) after which a cue parameter will begin its fade.
	WaitTime time.Time

	// Follow/Hang: Frequently, you will want a cue to start automatically after the previous cue has begun or has
	// completed. Putting a follow time on a cue causes it to trigger the next cue at the specified interval after
	// the "Go" button has been pressed. For example, If cue #101 has a follow of four seconds, cue #102 will begin
	// four seconds after cue #101 has begun (even if cue #101 is not yet complete).
	FollowTime time.Time

	// A blocking cue prevents level changes from tracking through it and successive cues.
	Block bool
}

func NewCue(cueName string, cueInitializer func()) {
	// TODO - log debug that a cue was created with cueName
	cueInitializer()
}
