package main

import (
	"math"
)

// g.sawtoothWave = function (v, min, max, period, offset) {
//     if (min === undefined) min = -1;
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
// };

// func (e *Effect) Update(deltaTime float64, value float64) float64 {
// 	if e.paused {
// 		return e.value
// 	}

// 	func InOutQuad(t float64) float64 {
// 		if t < 0.5 {
// 			return 2 * t * t
// 		} else {
// 			t = 2*t - 1
// 			return -0.5 * (t*(t-2) - 1)
// 		}
// 	}

var TWO_PI = math.Pi * 2

func SawToothWave(v, min, max, period, offset float64) float64 {
	amplitude := float64((max - min) / 2)
	frequency := TWO_PI / period
	phase := 0.0
	time := v + offset
	if math.Mod(time, period) != 0.0 {
		phase = math.Mod((time * frequency), TWO_PI)
	}
	if phase < 0 {
		phase += TWO_PI
	}
	return 2*(phase/TWO_PI)*amplitude + min
}

type Spring struct {
	posPosCoef, posVelCoef float64
	velPosCoef, velVelCoef float64
}

type SawToothEffect struct {
	amplitude float64
	frequency float64
	phase     float64
	time      float64
}

func NewSawToothEffect() (st SawToothEffect) {

}
