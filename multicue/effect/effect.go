package effect

import (
	"math"
	"time"
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

// var TWO_PI = float64(math.Pi * 2)
const TWO_PI = (2 * math.Pi)

// FPS returns a time delta for a given number of frames per second. This
// value can be used as the time delta when initializing a Spring. Note that
// game engines often provide the time delta as well, which you should use
// instead of this function, if possible.
//
// Example:
//
//	spring := NewSpring(FPS(60), 5.0, 0.2)
func FPS(n int) float64 {
	return (time.Second / time.Duration(n)).Seconds()
}

// Effect interface defines the methods that should be implemented by an effect.
type Interface interface {
	GetFixtureNames() []string
	GetFixtureAttrs() []string
	Update(t time.Time) float64
}

type Effect struct {
	// FixtureNames is a list of fixtures to apply the effect to.
	FixtureNames []string

	// FixtureAttrs is a list of fixture attributes to apply the effect to.
	FixtureAttrs []string

	// Oscillator determines which oscillator to use in order to apply the effect.
	Oscillator Oscillator

	// Wave is an effect offset that determines how many times to apply an oscillator.
	Wave int

	// Step is an effect offset that determines how many fixtures to apply the effect to at a time.
	Step int

	StartTime time.Time

	min float64
	max float64

	loop   bool    // Whether the effect loops
	paused bool    // Whether the effect is paused
	value  float64 // current value
}

type Oscillator struct {
	Swing      int
	Speed      int
	Multiplier int

	// swing or amplitude is the amount of oscillation to be applied to the effect.
	amplitude float64
	frequency float64
	//phase     float64
	period float64
	time   float64

	// ShapeFn determines the shape of the waveform.
	ShapeFn ShapeFn
}

type ShapeFn func(float64, float64) float64

func NewSawToothOsc() Oscillator {
	osc := Oscillator{}
	osc.period = 1.0
	// Set the frequency of the sine wave (in Hz)
	osc.frequency = 0.5 // 0.5 Hz for a slow effect
	osc.ShapeFn = sawtoothFunc
	return osc
}

func NewSineWaveOsc() Oscillator {
	osc := Oscillator{}
	osc.period = 1.0
	// Set the frequency of the sine wave (in Hz)
	osc.frequency = 0.5 // 0.5 Hz for a slow effect
	osc.ShapeFn = sineWaveFunc
	return osc
}

func (e Effect) GetFixtureNames() []string {
	return e.FixtureNames
}

func (e Effect) GetFixtureAttrs() []string {
	return e.FixtureAttrs
}

func (e Effect) Update(t time.Time) float64 {
	//val := 2.0*(phase*(1.0/tau)) - 1.0
	//val := 2.0*(value*(1.0/TWO_PI)) - 1.0
	//return val

	// Calculate the oscillator value at time t
	value := e.Oscillator.ShapeFn(t.Sub(e.StartTime).Seconds(), e.Oscillator.frequency)

	// TODO - clamp the value to the min and max values

	// Calculate the sawtooth value at time t
	//value := sawtooth(t.Sub(ste.startTime).Seconds())
	return value
}

type BaseEffect struct {
	startTime time.Time
	Time      float64 // Total running time

	Duration float64

	Target float64

	// Animation speed multiplier
	Speed float64

	min float64
	max float64

	loop   bool    // Whether the effect loops
	paused bool    // Whether the effect is paused
	value  float64 // current value
}

type SawToothEffect struct {
	BaseEffect
	amplitude float64
	//frequency float64
	//phase     float64
	period float64
	time   float64
}

type SineWaveEffect struct {
	BaseEffect
	amplitude float64
	//frequency float64
	//phase     float64
	period float64
	time   float64
}

func NewSawToothEffect(deltaTime float64) SawToothEffect {
	//ste.min = -1.0
	ste := SawToothEffect{}
	ste.startTime = time.Now()
	ste.min = 0.0
	ste.max = 1.0
	ste.amplitude = float64((ste.max - ste.min) / 2)
	ste.period = 1.0
	ste.time = deltaTime
	//ste.phase = 0.0
	//ste.Speed = 1.0
	return ste
}

func NewSineWaveEffect(deltaTime float64) SineWaveEffect {
	swe := SineWaveEffect{}
	swe.startTime = time.Now()
	swe.time = deltaTime
	//ste.phase = 0.0
	//ste.Speed = 1.0
	return swe
}

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

// The sawtooth curve can be used to modulate the intensity or other parameters of the light.
// Calculate the value of the sawtooth wave at each beat.
func sawtoothFunc(t float64, frequency float64) float64 {
	return 2 * (t/math.Pi - math.Floor(frequency+t/math.Pi))
}

func sineWaveFunc(t float64, frequency float64) float64 {
	// Sine wave formula: A * sin(2πft + φ)
	// A = amplitude, f = frequency, t = time, φ = phase shift
	// Here, we assume amplitude=1 and phase shift=0 for simplicity
	return math.Sin(2 * math.Pi * frequency * t)
}

// func (ste SawToothEffect) Update(value float64) float64 {
// 	frequency := TWO_PI / ste.period
// 	var phase float64

// 	if math.Mod(value, ste.period) != 0.0 {
// 		phase = math.Mod((value * frequency), TWO_PI)
// 	}

// 	if phase < 0.0 {
// 		phase += TWO_PI
// 	}

// 	val := 2*(phase/TWO_PI)*ste.amplitude + ste.min
// 	return val
// }

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

// func (ste SawToothEffect) Update(deltaTime float64, value float64) float64 {
// 	// if e.paused {
// 	// 	return e.value
// 	// }

// 	//e.Time = e.Time + deltaTime*e.Speed
// 	//ste.time := e.time + deltaTime*e.Speed
// 	//ste.time = ste.time + deltaTime

// 	frequency := TWO_PI / ste.period
// 	phase := 0.0

// 	if math.Mod(ste.time, value) != 0.0 {
// 		phase = math.Mod((deltaTime * frequency), TWO_PI)
// 	}

// 	if phase < 0.0 {
// 		phase += TWO_PI
// 	}

// 	return 2*(phase/TWO_PI)*ste.amplitude + ste.min

// 	//     if (time % period !== 0) {
// 	//         phase = (time * frequency) % TWO_PI;
// 	//     }
// 	//     if (phase < 0) { phase += TWO_PI; }
// 	//     return 2 * (phase / TWO_PI) * amplitude + min;
// }

func clamp(t, minVal, maxVal float64) float64 {
	minVal, maxVal = min(minVal, maxVal), max(minVal, maxVal)
	return max(min(t, maxVal), minVal)
}
