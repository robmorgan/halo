package effect

import (
	"fmt"

	"github.com/fogleman/ease"
)

type Effect struct {
	// The type of the effect
	Type string

	Time int
}

// Create a new Effect of type t for the specified time
func NewEffect(t string, time int) *Effect {
	return &Effect{
		Type: t,
		Time: time,
	}
}

func (e *Effect) Update(value float64, target float64) float64 {
	// TODO - In the future support switching to different easing functions. For now just hard-code the InQuart function.
	fmt.Printf("Value is %.2f and target is %.2f\n", value, target)
	return ease.InQuart(value / target)
}

// TODO - work on property animation: https://developer.android.com/reference/android/animation/ObjectAnimator
// https://developer.android.com/guide/topics/graphics/prop-animation#object-animator
// https://github.com/charmbracelet/harmonica
// https://github.com/aroffringa/glight/blob/master/theatre/effect.h
