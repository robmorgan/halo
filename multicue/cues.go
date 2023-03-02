package main

import "time"

var cues = []Cue{
	{
		name:     "Cue 1",
		FadeTime: time.Second * 10,
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
}

func getCues() []Cue {
	c := cues
	copy(c, cues)
	return c
}
