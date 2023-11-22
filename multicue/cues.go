package main

import (
	"log/slog"
	"time"

	"github.com/robmorgan/halo/fixture"
	"github.com/robmorgan/halo/multicue/effect"
	"github.com/robmorgan/halo/profile"
	"github.com/robmorgan/halo/utils"
)

var cues = []Cue{
	{
		Name:     "Cue 1",
		FadeTime: time.Second * 5,
		Actions: []CueAction{
			{
				FixtureNames: []string{"left_top_par", "right_top_par"},
				Effects: []effect.Effect{
					{
						FixtureNames: []string{"left_top_par", "right_top_par"},
						FixtureAttrs: []string{profile.ChannelTypeIntensity},
						StartTime:    time.Now(),
						Oscillator:   effect.NewSawToothOsc(),
					},
				},
			},
		},
	},
	{
		Name:     "Cue 2 - Cycle PARs",
		FadeTime: time.Second * 10,
		Actions: []CueAction{
			{
				FixtureNames: []string{"left_top_par", "right_top_par"},
				Effects: []effect.Effect{
					{
						FixtureNames: []string{"left_top_par", "right_top_par"},
						FixtureAttrs: []string{profile.ChannelTypeIntensity},
						StartTime:    time.Now(),
						Oscillator:   effect.NewSineWaveOsc(),
					},
				},
			},
		},
	},
}

type Cue struct {
	Name string

	// A cue's "time" is a measure of how long it takes the cue to complete, once it has been executed. Depending upon
	// the console, time(s), entered in minutes and seconds, can be entered for the cue as a whole or, individually,
	// for transitions in focus, intensity (up and/or down), and color, as well as for individual channels. Time (or
	// delay) applied to individual channels is called, "discrete" timing.
	FadeTime time.Duration

	Actions []CueAction

	progress float64
}

// CueAction is an action within a Cue to be executed simultaneously.
type CueAction struct {
	ID           int64
	FixtureNames []string            // list of fixtures to apply the action to
	NewState     fixture.TargetState // desired base target state for the fixtures
	Effects      []effect.Effect     // the target effects to apply
}

// GetDuration returns the sum of frames in a cue
// func (c *Cue) GetDuration() time.Duration {
// 	totalDuration := time.Duration(0)
// 	for _, frame := range c.Frames {
// 		totalDuration += frame.GetDuration()
// 	}
// 	return totalDuration
// }

// TODO - this should be an update method and not return an individual effect value
// We need to ensure it can update a bunch of fixture values at the same time
func (c *Cue) RenderFrame(fixtureManager fixture.Manager, t time.Time) {

	// TODO - snapshot the current metronome state

	// render all cue actions
	for _, action := range c.Actions {
		// process all active effects
		//action.effectValue = action.Effect.Update(t)
		//return int(action.effectValue * 255)
		for _, effect := range action.Effects {
			effectVal := effect.Update(t)

			// you might need to clamp here
			clampVal := int(clamp(effectVal*255.0, 0.0, 255.0))

			// get a list of fixtures and attributes that are affected by this effect
			fixtureNames := effect.GetFixtureNames()
			//fixtureAttrs := effect.GetFixtureAttrs()

			// compute the new state
			newState := fixture.TargetState{
				// Set Red Property
				State: fixture.State{Intensity: clampVal, Strobe: 0, RGB: utils.GetRGBFromString("#FF0000")},
				//Duration: frameDuration,
				//TickInterval: fixture.TickIntervalFadeInterpolation,
			}

			// update the fixture states
			for _, fixtureName := range fixtureNames {
				if f := fixtureManager.GetByName(fixtureName); f != nil {
					go f.SetState(fixtureManager, newState)
				} else {
					slog.Error("Cannot find fixture by name", "name", fixtureName)
				}
			}
		}
	}
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
