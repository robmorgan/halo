package main

import (
	"time"

	"github.com/robmorgan/halo/cuelist"
	"github.com/robmorgan/halo/fixture"
	"github.com/robmorgan/halo/utils"
)

const LoveSensationAudioFile = "/Users/robbym/Dropbox/DJ Stuff/DJSLSummer2022/Music/15742477_Love Sensation_(Eddie Thoneick's Sensation Radio Mix).wav"

// Cue #2: Middle PARs off, Strobe top PARs
func getLoveSensationCue2() *cuelist.Cue {
	cue := cuelist.Cue{}

	// turn off fixtures
	offFixtures := []string{"left_middle_par", "right_middle_par"}
	cue.Frames = append(cue.Frames, getFrameToClearFixtures(offFixtures, time.Millisecond*30))

	// strobe fixtures
	fixtureList := []string{"left_top_par", "right_top_par", "left_middle_par", "right_middle_par", "left_bottom_par", "right_bottom_par"}
	duration := time.Second * 5
	numFrames := 1

	for x := 0; x < numFrames; x++ {
		frame := cuelist.Frame{}
		frameDuration := duration / time.Duration(numFrames)

		for y := 0; y < len(fixtureList); y++ {
			action := cuelist.FrameAction{}
			action.FixtureName = fixtureList[y]
			action.NewState = fixture.TargetState{
				// Set White Property
				State:    fixture.State{Intensity: 100, Strobe: 210, RGB: utils.GetRGBFromString("#FFFFFF")},
				Duration: frameDuration,
				//TickInterval: fixture.TickIntervalFadeInterpolation,
			}
			frame.Actions = append(frame.Actions, action)
		}
		cue.Frames = append(cue.Frames, frame)
	}

	return &cue
}

func getFrameToClearFixtures(fixtureList []string, duration time.Duration) cuelist.Frame {
	frame := cuelist.Frame{}
	for x := range fixtureList {
		action := cuelist.FrameAction{}
		action.FixtureName = fixtureList[x]
		action.NewState = fixture.TargetState{
			State:    fixture.State{Intensity: 0}, // TODO - do we want to reset more attributes?
			Duration: duration,
		}
		frame.Actions = append(frame.Actions, action)
	}

	return frame
}
