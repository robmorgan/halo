package scale

import "math"

func clamp(t, min, max float64) float64 {
	min, max = math.Min(min, max), math.Max(min, max)
	return math.Max(math.Min(t, max), min)
}

// ToUnitClamp returns a function that scales a number from the interval [rMin,rMax]
// to the unit interval ([0,1]), if the result falls outside [0,1], it is clamped
// to 0 or 1.
func ToUnitClamp(rMin, rMax float64) func(m float64) float64 {
	return Clamp(rMin, rMax, 0, 1)
}
