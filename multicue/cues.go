package main

import (
	"github.com/charmbracelet/bubbles/progress"
)

var cues = []Cue{
	Cue{
		name: "Cue 1",
	},
}

type Cue struct {
	name     string
	progress progress.Model
	// actions
	// effects
}

func getCues() []Cue {
	c := cues
	copy(c, cues)

	for i := range c {
		p := progress.New(
			progress.WithDefaultGradient(),
			progress.WithWidth(40),
			progress.WithoutPercentage(),
		)

		c[i].progress = p
	}

	return c
}
