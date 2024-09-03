package effect

// SawtoothShapeFunc represents the shape function for a sawtooth wave.
type SawtoothShapeFunc func(phase float64) float64

// BuildFixedSawtoothShapeFn returns the shape function for a sawtooth wave in a fixed direction.
func BuildFixedSawtoothShapeFn(down bool) SawtoothShapeFunc {
	if down {
		return func(phase float64) float64 {
			return 1.0 - phase
		}
	}
	return func(phase float64) float64 {
		return phase
	}
}
