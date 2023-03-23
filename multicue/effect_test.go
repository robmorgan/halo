package main

import (
	"fmt"
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestSawToothWave(t *testing.T) {
	t.Parallel()

	//if (min === undefined) min = -1;
	//     if (max === undefined) max = 1;
	//     if (period === undefined) period = 1;
	//     if (offset === undefined) offset =  0;
	//     var amplitude = (max - min) / 2,
	//         frequency = TWO_PI / period,
	//         phase = 0,
	//         time = v + offset;
	//     if (time % period !== 0) {
	//         phase = (time * frequency) % TWO_PI;
	//     }
	//     if (phase < 0) { phase += TWO_PI; }
	//     return 2 * (phase / TWO_PI) * amplitude + min;

	min := -1.0
	max := 1.0
	period := 1.0
	offset := 0.0
	v := 0.1

	for i := 0; i < 100; i++ {
		v = SawToothWave(v, min, max, period, offset)
		fmt.Println(v)
	}

	assert.Equal(t, 1.0, v)
}

// func TestEffects(t *testing.T) {
// 	t.Parallel()

// 	testCases := []struct {
// 		easingFunc ease.Function
// 		speed      float64
// 		duration   float64
// 		ticks      int
// 		expected   float64
// 	}{
// 		{ease.Linear, 1.0, 1.0, 40, 1.0},
// 		{ease.Linear, 1.0, 1.0, 19, 0.5},
// 		{ease.InOutQuart, 1.0, 1.0, 40, 1.0},
// 		{ease.InOutQuart, 1.0, 1.0, 10, 0.32},
// 	}

// 	for _, testCase := range testCases {
// 		delta := FPS(40)
// 		var newVal float64

// 		effect := NewEffect(testCase.easingFunc, testCase.duration, testCase.speed)
// 		for i := 0; i < testCase.ticks; i++ {
// 			oldVal := newVal
// 			newVal = effect.Update(delta, newVal)
// 			fmt.Printf("count=%d delta=%.7f speed=%.7f duration=%.7f oldVal=%.7f newVal=%.7f\n", i+1, delta, testCase.speed, testCase.duration, oldVal, newVal)
// 		}

// 		assert.LessOrEqual(t, newVal, testCase.expected)
// 	}
// }
