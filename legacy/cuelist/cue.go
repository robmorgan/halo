package cuelist

import (
	"time"

	"github.com/robmorgan/halo/fixture"
)

// I've borrowed heavily from: http://www.stagelightingprimer.com/index.html?slfs-control.html&2

const (
	statusEnqueued  string = "enqueued"
	statusActive           = "active"
	statusProcessed        = "processed"
)

// CueList stores a list of cues and can play them back
type Cue struct {
	ID int64

	// The name or label associated with the cue
	Name string

	Status string

	// Cue frames
	Frames []Frame

	// A cue's "time" is a measure of how long it takes the cue to complete, once it has been executed. Depending upon
	// the console, time(s), entered in minutes and seconds, can be entered for the cue as a whole or, individually,
	// for transitions in focus, intensity (up and/or down), and color, as well as for individual channels. Time (or
	// delay) applied to individual channels is called, "discrete" timing.
	FadeTime time.Time

	// The (optional) length of time (in seconds, after pressing the "Go" button) after which a cue parameter will begin its fade.
	WaitTime time.Duration

	// Follow/Hang: Frequently, you will want a cue to start automatically after the previous cue has begun or has
	// completed. Putting a follow time on a cue causes it to trigger the next cue at the specified interval after
	// the "Go" button has been pressed. For example, If cue #101 has a follow of four seconds, cue #102 will begin
	// four seconds after cue #101 has begun (even if cue #101 is not yet complete).
	FollowTime time.Time

	// A blocking cue prevents level changes from tracking through it and successive cues.
	Block bool

	StartedAt    time.Time
	FinishedAt   time.Time
	RealDuration time.Duration

	cueInitializerFunc func()
}

// Frame is a single 'animation frame' of a Cue
type Frame struct {
	Actions []FrameAction
	ID      int64
}

// FrameAction is an action within a Cue(Frame) to be executed simultaneously
type FrameAction struct {
	NewState    fixture.TargetState
	ID          int64
	FixtureName string
	Fixture     fixture.Interface
	// TODO - add way to have a noop action (to block aka wait for time)
}

// Go plays the next cue
func (c *Cue) Go() bool {
	c.cueInitializerFunc()
	return true
}

// GetDuration returns the sum of frames in a cue
func (c *Cue) GetDuration() time.Duration {
	totalDuration := time.Duration(0)
	for _, frame := range c.Frames {
		totalDuration += frame.GetDuration()
	}
	return totalDuration
}

// GetDuration returns the longest lasting Action within a CueFrame
func (cf *Frame) GetDuration() time.Duration {
	longest := time.Duration(0)
	for _, action := range cf.Actions {
		if d := action.NewState.Duration; d > longest {
			longest = d
		}
	}
	return longest
}
