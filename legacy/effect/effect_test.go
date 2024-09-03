package effect

import (
	"fmt"
	"testing"

	"github.com/fogleman/ease"
	"github.com/stretchr/testify/assert"
)

func TestEffects(t *testing.T) {
	t.Parallel()

	testCases := []struct {
		easingFunc ease.Function
		speed      float64
		duration   float64
		ticks      int
		expected   float64
	}{
		{ease.Linear, 1.0, 1.0, 40, 1.0},
		{ease.Linear, 1.0, 1.0, 19, 0.5},
		{ease.InOutQuart, 1.0, 1.0, 40, 1.0},
		{ease.InOutQuart, 1.0, 1.0, 10, 0.32},
	}

	for _, testCase := range testCases {
		delta := FPS(40)
		var newVal float64

		effect := NewEffect(testCase.easingFunc, testCase.duration, testCase.speed)
		for i := 0; i < testCase.ticks; i++ {
			oldVal := newVal
			newVal = effect.Update(delta, newVal)
			fmt.Printf("count=%d delta=%.7f speed=%.7f duration=%.7f oldVal=%.7f newVal=%.7f\n", i+1, delta, testCase.speed, testCase.duration, oldVal, newVal)
		}

		assert.LessOrEqual(t, newVal, testCase.expected)
	}
}
