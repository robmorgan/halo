package main

import (
	"errors"
	"time"

	"github.com/robmorgan/halo/cuelist"
	"github.com/robmorgan/halo/fixture"
	"github.com/robmorgan/halo/utils"
)

// Create a cue with a single frame thats designed to clear out fixtures
func clearFixtures(fixtureList []string, duration time.Duration) *cuelist.Cue {
	cue := cuelist.Cue{}

	frame := cuelist.Frame{}
	for x := range fixtureList {
		action := cuelist.FrameAction{}
		action.FixtureName = fixtureList[x]
		action.NewState = fixture.TargetState{
			State:    fixture.State{Intensity: 0},
			Duration: duration,
		}
		frame.Actions = append(frame.Actions, action)
	}
	cue.Frames = append(cue.Frames, frame)

	return &cue
}

func flashPARs(fixtureList []string, state fixture.State, timeDuration string, numFrames int) (*cuelist.Cue, error) {
	cue := cuelist.Cue{}
	duration, err := time.ParseDuration(timeDuration)
	if err != nil {
		return nil, err
	}

	var fixtureIndex int = 0
	for x := 0; x < numFrames; x++ {
		frame := cuelist.Frame{}
		frameDuration := duration / time.Duration(numFrames)

		for y := 0; y < len(fixtureList); y++ {
			action := cuelist.FrameAction{}
			action.FixtureName = fixtureList[y]

			action.NewState = fixture.TargetState{
				State:    state,
				Duration: frameDuration,
			}

			frame.Actions = append(frame.Actions, action)
		}

		fixtureIndex++
		if fixtureIndex > len(fixtureList)-1 {
			fixtureIndex = 0
		}

		cue.Frames = append(cue.Frames, frame)
	}

	return &cue, nil
}

// The number of frames has to be greater than equal to the size of the fixture list.
func cycleFixtureStates(fixtureList []string, stateA fixture.State, stateB fixture.State, timeDuration string, numFrames int) (*cuelist.Cue, error) {
	cue := cuelist.Cue{}
	duration, err := time.ParseDuration(timeDuration)
	if err != nil {
		return nil, err
	}

	var fixtureIndex int = 0
	for x := 0; x < numFrames; x++ {
		frame := cuelist.Frame{}
		frameDuration := duration / time.Duration(numFrames)

		for y := 0; y < len(fixtureList); y++ {
			action := cuelist.FrameAction{}
			action.FixtureName = fixtureList[y]

			action.NewState = fixture.TargetState{
				State:    stateA,
				Duration: frameDuration,
			}

			if y == fixtureIndex {
				action.NewState = fixture.TargetState{
					State:    stateB,
					Duration: frameDuration,
				}
			}

			frame.Actions = append(frame.Actions, action)
		}

		fixtureIndex++
		if fixtureIndex > len(fixtureList)-1 {
			fixtureIndex = 0
		}

		cue.Frames = append(cue.Frames, frame)
	}

	return &cue, nil
}

func processMiddleParCycleCommand(timeStr string, numFrames int) (*cuelist.Cue, error) {
	fixtureList := []string{"left_middle_par", "right_middle_par"}
	duration, err := time.ParseDuration(timeStr)
	if err != nil {
		return nil, err
	}

	cue := cuelist.Cue{}

	for x := 0; x < numFrames; x++ {
		frame := cuelist.Frame{}
		frameDuration := duration / time.Duration(numFrames)

		var leftInt int
		var rightInt int

		if x%2 == 0 {
			// even
			leftInt = 200
			rightInt = 0
		} else {
			// odd
			leftInt = 0
			rightInt = 200
		}

		leftAction := cuelist.FrameAction{}
		leftAction.FixtureName = fixtureList[0]
		leftAction.NewState = fixture.TargetState{
			State:    fixture.State{Intensity: leftInt, RGB: utils.GetRGBFromString("white")},
			Duration: frameDuration,
		}
		frame.Actions = append(frame.Actions, leftAction)

		rightAction := cuelist.FrameAction{}
		rightAction.FixtureName = fixtureList[1]
		rightAction.NewState = fixture.TargetState{
			State:    fixture.State{Intensity: rightInt, RGB: utils.GetRGBFromString("white")},
			Duration: frameDuration,
		}
		frame.Actions = append(frame.Actions, rightAction)

		cue.Frames = append(cue.Frames, frame)
	}

	return &cue, nil
}

// e.g. cycle(c1+c2+c3+c4+c5+c6:500ms)
func processCycleCommand(timeStr string) (*cuelist.Cue, error) {
	cue := cuelist.Cue{}

	fixtureList := []string{"left_middle_par", "right_middle_par"}
	duration, err := time.ParseDuration(timeStr)
	if err != nil {
		return nil, err
	}
	for x := range fixtureList {
		frame := cuelist.Frame{}
		for y := 0; y < len(fixtureList); y++ {
			action := cuelist.FrameAction{}
			action.FixtureName = fixtureList[y]

			action.NewState = fixture.TargetState{
				State:    fixture.State{Intensity: 200, RGB: utils.GetRGBFromString("#0000FF")},
				Duration: duration,
			}
			if x == y {
				action.NewState = fixture.TargetState{
					State:    fixture.State{Intensity: 200, RGB: utils.GetRGBFromString("#FF0000")},
					Duration: duration,
				}
			}

			frame.Actions = append(frame.Actions, action)
		}
		cue.Frames = append(cue.Frames, frame)
	}

	return &cue, nil
}

func processTopParsCommand(timeStr string) (*cuelist.Cue, error) {
	fixtureList := []string{"left_top_par", "right_top_par"}

	cue := cuelist.Cue{}
	frame := cuelist.Frame{}

	for x := range fixtureList {
		action := cuelist.FrameAction{}
		action.FixtureName = fixtureList[x]
		duration, err := time.ParseDuration(timeStr)
		if err != nil {
			return nil, errors.New("invalid time")
		}
		action.NewState = fixture.TargetState{
			State:    fixture.State{Intensity: 200, RGB: utils.GetRGBFromString("white")},
			Duration: duration,
		}
		frame.Actions = append(frame.Actions, action)
	}
	cue.Frames = append(cue.Frames, frame)

	return &cue, nil
}

// e.g. cycle(c1+c2+c3+c4+c5+c6:500ms)
func processCycleCommandSpots(timeStr string) (*cuelist.Cue, error) {
	cue := cuelist.Cue{}

	fixtureList := []string{"left_spot", "right_spot"}
	duration, err := time.ParseDuration(timeStr)
	if err != nil {
		return nil, err
	}
	for x := range fixtureList {
		frame := cuelist.Frame{}
		for y := 0; y < len(fixtureList); y++ {
			action := cuelist.FrameAction{}
			action.FixtureName = fixtureList[y]

			action.NewState = fixture.TargetState{
				State:    fixture.State{Intensity: 200, Tilt: 100, RGB: utils.GetRGBFromString("#0000FF")},
				Duration: duration,
			}
			if x == y {
				action.NewState = fixture.TargetState{
					State:    fixture.State{Intensity: 200, Tilt: 100, RGB: utils.GetRGBFromString("#FF0000")},
					Duration: duration,
				}
			}

			frame.Actions = append(frame.Actions, action)
		}
		cue.Frames = append(cue.Frames, frame)
	}

	return &cue, nil
}

// e.g. cycle(c1+c2+c3+c4+c5+c6:500ms)
func processCycleCommandBeams(timeStr string) (*cuelist.Cue, error) {
	cue := cuelist.Cue{}

	fixtureList := []string{"left_beam_bar", "right_beam_bar"}
	duration, err := time.ParseDuration(timeStr)
	if err != nil {
		return nil, err
	}
	for x := range fixtureList {
		frame := cuelist.Frame{}
		for y := 0; y < len(fixtureList); y++ {
			action := cuelist.FrameAction{}
			action.FixtureName = fixtureList[y]

			action.NewState = fixture.TargetState{
				State:    fixture.State{Intensity: 200, Tilt: 100, RGB: utils.GetRGBFromString("#0000FF")},
				Duration: duration,
			}
			if x == y {
				action.NewState = fixture.TargetState{
					State:    fixture.State{Intensity: 200, Tilt: 100, RGB: utils.GetRGBFromString("#FF0000")},
					Duration: duration,
				}
			}

			frame.Actions = append(frame.Actions, action)
		}
		cue.Frames = append(cue.Frames, frame)
	}

	return &cue, nil
}

// e.g. cycle(c1+c2+c3+c4+c5+c6:500ms)
func processCycleCommandWashes(timeStr string) (*cuelist.Cue, error) {
	cue := cuelist.Cue{}

	fixtureList := []string{"left_wash", "right_wash"}
	duration, err := time.ParseDuration(timeStr)
	if err != nil {
		return nil, err
	}
	for x := range fixtureList {
		frame := cuelist.Frame{}
		for y := 0; y < len(fixtureList); y++ {
			action := cuelist.FrameAction{}
			action.FixtureName = fixtureList[y]

			action.NewState = fixture.TargetState{
				State:    fixture.State{Intensity: 255, Pan: 38, Tilt: 55, RGB: utils.GetRGBFromString("#0000FF")},
				Duration: duration,
			}
			if x == y {
				action.NewState = fixture.TargetState{
					State:    fixture.State{Intensity: 255, Pan: 38, Tilt: 55, RGB: utils.GetRGBFromString("#FF0000")},
					Duration: duration,
				}
			}

			frame.Actions = append(frame.Actions, action)
		}
		cue.Frames = append(cue.Frames, frame)
	}

	return &cue, nil
}
