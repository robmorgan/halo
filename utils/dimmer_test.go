package utils

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestGetDimmerFadeValue(t *testing.T) {
	t.Parallel()

	target := 250
	step := 15
	numSteps := 30
	expected := 129

	value := GetDimmerFadeValue(target, step, numSteps)
	require.Equal(t, expected, value)
}
