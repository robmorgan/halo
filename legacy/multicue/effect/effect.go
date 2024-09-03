package effect

import (
	"math"
	"time"

	"github.com/robmorgan/halo/rhythm"
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

	// WaveOffset is an effect offset that determines how many times to apply an oscillator.
	WaveOffset int

	// StepOffset is an effect offset that determines how many fixtures to apply the effect to at a time.
	StepOffset int
	stepIndex  int
	stepCount  int
	stepTime   time.Time

	StartTime time.Time

	// targetFixtureNames is a list of the current fixture names that the effect is being applied to.
	targetFixtureNames []string

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

// NewEffect creates a new effect with the given parameters.
func NewEffect(fixtureNames []string, fixtureAttrs []string, stepOffset int, oscillator Oscillator) *Effect {
	return &Effect{
		FixtureNames:       fixtureNames,
		FixtureAttrs:       fixtureAttrs,
		targetFixtureNames: fixtureNames,
		StartTime:          time.Now(),
		StepOffset:         stepOffset,
		Oscillator:         oscillator,
	}
}

// GetFixtureNames returns the list of fixture names to apply the effect to.
func (e Effect) GetFixtureNames() []string {
	return e.FixtureNames
}

func (e *Effect) recalculateTargetFixtureNames(currentTime time.Time) {
	// First check if any step or wave offsets are being used and return if not
	if e.StepOffset == 0 || len(e.FixtureNames) == e.StepOffset {
		return
	}

	// Check if it's time to move to the next fixture
	// TODO - calculate the effect length based off the metronome values
	if currentTime.Sub(e.stepTime) > time.Duration(e.StepOffset)*time.Millisecond {
		e.stepIndex++
		e.stepTime = currentTime
	}

	targetFixtureNames := make([]string, 0)
	// TODO - this will cause an out of range panic if e.Step is greater than the number of fixture names.
	// We need to clamp or do some magic foo here to prevent this.
	for i := e.stepIndex; i < e.StepOffset+1; i++ {
		targetFixtureNames = append(targetFixtureNames, e.FixtureNames[i])
	}
	e.targetFixtureNames = targetFixtureNames

	// if we've reached the end of available fixtures, go back to the start
	if e.stepIndex >= len(e.FixtureNames) {
		e.stepIndex = 0
	}
}

// GetTargetFixtureNames returns the list of fixture names to apply the effect to. If the Wave or Step values are set,
// then this might return a subset of the fixture names.
func (e Effect) GetTargetFixtureNames() []string {
	return e.targetFixtureNames
}

// fNames := make([]string, 0)
// for _, fixtureName := range e.FixtureNames {
// }
// fixtureIndex := e.GetFixtureIndex()

// if effect.Step == 0 || len(fixtureNames) == effect.Step {
// 	for _, fixtureName := range fixtureNames {
// 		if f := fixtureManager.GetByName(fixtureName); f != nil {
// 			go f.SetState(fixtureManager, newState)
// 		} else {
// 			slog.Error("Cannot find fixture by name", "name", fixtureName)
// 		}
// 	}
// } else {
// 	// Otherwise apply the new state using the step offset value.
// 	// TODO - we only support 1 fixture at a time at the moment using this logic.
// 	stepIndex := effect.GetStepIndex()
// 	fixtureName := fixtureNames[stepIndex]
// 	if f := fixtureManager.GetByName(fixtureName); f != nil {
// 		go f.SetState(fixtureManager, newState)
// 	} else {
// 		slog.Error("Cannot find fixture by name", "name", fixtureName)
// 	}

// 	// increment the step index or reset it if necessary
// 	if len(fixtureNames) == stepIndex+1 {
// 		effect.SetStepIndex(0)
// 	} else {
// 		effect.SetStepIndex(stepIndex + 1)
// 	}
// }
// }

func (e Effect) GetFixtureAttrs() []string {
	return e.FixtureAttrs
}

func (e *Effect) GetStepIndex() int {
	return e.stepIndex
}

func (e *Effect) SetStepIndex(idx int) {
	e.stepIndex = idx
}

func (e Effect) Update(snapshot rhythm.Snapshot) float64 {
	//val := 2.0*(phase*(1.0/tau)) - 1.0
	//val := 2.0*(value*(1.0/TWO_PI)) - 1.0
	//return val

	snapshot.In

	t := currentTime.Sub(e.StartTime).Seconds()

	// Check if it's time to move to the next fixture
	e.recalculateTargetFixtureNames(currentTime)
	// // TODO - might need to calculate the effect time better
	// if currentTime.Sub(e.StartTime) > time.Duration(e.StepOffset)*time.Millisecond {

	// }

	// Calculate the oscillator value at time t
	value := e.Oscillator.ShapeFn(t, e.Oscillator.frequency)

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
