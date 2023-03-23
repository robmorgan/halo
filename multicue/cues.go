package main

import "time"

var cues = []Cue{
	{
		name:     "Cue 1",
		FadeTime: time.Second * 10,
	},
	{
		name:     "Cue 2",
		FadeTime: time.Second * 5,
	},
}

type Cue struct {
	name string
	// actions
	// effects

	// A cue's "time" is a measure of how long it takes the cue to complete, once it has been executed. Depending upon
	// the console, time(s), entered in minutes and seconds, can be entered for the cue as a whole or, individually,
	// for transitions in focus, intensity (up and/or down), and color, as well as for individual channels. Time (or
	// delay) applied to individual channels is called, "discrete" timing.
	FadeTime time.Duration

	Fixtures []string
	Effect   string // the target effect to apply to the fixtures

	progress float64
}

// GetDuration returns the sum of frames in a cue
// func (c *Cue) GetDuration() time.Duration {
// 	totalDuration := time.Duration(0)
// 	for _, frame := range c.Frames {
// 		totalDuration += frame.GetDuration()
// 	}
// 	return totalDuration
// }

func (c *Cue) RenderFrame() {

}

func (c *Cue) Progress() float64 {
	c.progress += 0.1
	return c.progress
}

func getCues() []Cue {
	c := cues
	copy(c, cues)
	return c
}
