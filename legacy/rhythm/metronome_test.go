package rhythm

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestMetronome(t *testing.T) {
	t.Parallel()

	// Create a new metronome with a default of 120 bpm
	m := NewMetronome()

	// The beat interval should be every 500ms
	assert.Equal(t, 500.0, m.GetBeatInterval())

	// Try to change the tempo
	m.SetTempo(128.0)

	// The beat interval should change to  be
	assert.Equal(t, 468.75, m.GetBeatInterval())
}
