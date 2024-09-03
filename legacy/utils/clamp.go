package utils

import "math"

func clamp(t, min, max float64) float64 {
	min, max = math.Min(min, max), math.Max(min, max)
	return math.Max(math.Min(t, max), min)
}
