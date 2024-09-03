package utils

func GetDimmerFadeValue(target, step, numSteps int) int {
	progress := float64(step) / float64(numSteps-1)
	if progress == 1 {
		return target
	}

	out := clamp(progress*float64(target), 0, 255)
	return int(out)
}
