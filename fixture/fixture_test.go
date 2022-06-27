package fixture

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNewFixture(t *testing.T) {
	t.Parallel()

	fix := NewFixture(1, 138, 8, map[int]*Channel{
		1: {
			Type:       TypeIntensity,
			Address:    1,
			Resolution: 1,
		},
		2: {
			Type:       TypeColorRed,
			Address:    2,
			Resolution: 1,
		},
	})

	// set some values
	fix.SetIntensity(0.5)
	fix.SetColor(0.8)

	intensity, err := fix.GetIntensity()
	require.NoError(t, err)

	color, err := fix.GetColor()
	require.NoError(t, err)

	assert.Equal(t, 0.5, intensity)
	assert.Equal(t, 0.8, color)
}

func TestNeedsUpdate(t *testing.T) {
	t.Parallel()

	fix := NewFixture(1, 1, 1, nil)

	// set a value
	fix.SetIntensity(1.0)
	require.True(t, fix.NeedsUpdate())

	// reset fixture
	fix.HasUpdated()
	require.False(t, fix.NeedsUpdate())
}
