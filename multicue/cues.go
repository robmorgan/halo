package main

var cues = []Cue{
	Cue{
		name: "Cue 1",
	},
}

type Cue struct {
	name string
	// actions
	// effects
}

func getCues() []Cue {
	c := cues
	copy(c, cues)
	return c
}
