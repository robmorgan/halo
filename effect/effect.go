package effect

import (
	"fmt"
	"math"
	"time"

	"github.com/fogleman/ease"
)

// FPS returns a time delta for a given number of frames per second. This
// value can be used as the time delta when initializing a Spring. Note that
// game engines often provide the time delta as well, which you should use
// instead of this function, if possible.
//
// Example:
//
//     spring := NewSpring(FPS(60), 5.0, 0.2)
//
func FPS(n int) float64 {
	return (time.Second / time.Duration(n)).Seconds()
}

type Effect struct {
	// The easing function to use
	EasingFunc ease.Function

	// Total running time
	Time float64

	Duration float64

	Target float64

	// Animation speed multiplier
	Speed float64

	loop   bool    // Whether the effect loops
	paused bool    // Whether the effect is paused
	value  float64 // current value
}

// NewEffect creates and returns a pointer to a new Effect object of type t for the specified time.
func NewEffect(easingFunc ease.Function, duration float64, speed float64) *Effect {
	return &Effect{
		EasingFunc: easingFunc,
		Time:       0.000001,
		Target:     1.0,
		Duration:   duration,
		Speed:      speed,
	}
}

func (e *Effect) Update(deltaTime float64, value float64) float64 {
	if e.paused {
		return e.value
	}

	e.Time = e.Time + deltaTime*e.Speed

	// TODO - you probably want to port these equations which work on current time and duration
	// it will be easier to see the effect progress over time. Here is a good example of equations
	// to port: https://github.com/node-dmx/dmx/blob/master/easing.js.

	// get the next value from the easing function
	// At the moment we bound because the duration + speed can produce some really funky values
	e.value = math.Max(0.0, math.Min(e.Target, e.EasingFunc(e.Time)))

	// check if input is greater than the maximum
	if e.Time > e.Duration {
		if e.loop {
			e.Time = e.Time - e.Duration
			fmt.Printf("resetting animation\n")
		} else {
			fmt.Printf("reducing time\n")
			e.Time = e.Duration - 0.000001
			e.paused = true
			e.value = 1.0 // TODO - we don't support e.Target yet
		}
	}

	fmt.Printf("time=%.7f deltaTime=%.7f duration=%.7f speed=%.7f value=%.7f newValue=%.7f\n", e.Time, deltaTime, e.Duration, e.Speed, value, e.value)

	return e.value
}

// TODO - work on property animation: https://developer.android.com/reference/android/animation/ObjectAnimator
// https://developer.android.com/guide/topics/graphics/prop-animation#object-animator
// https://github.com/charmbracelet/harmonica
// https://github.com/aroffringa/glight/blob/master/theatre/effect.h
