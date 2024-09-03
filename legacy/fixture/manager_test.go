package fixture

import (
	"testing"

	"github.com/robmorgan/halo/config"
	"github.com/stretchr/testify/require"
)

// func TestGetType(t *testing.T) {
// 	tests := []struct {
// 		input    Light
// 		expected string
// 	}{
// 		{&DMXLight{}, TypeDMX},
// 		{&HueLight{}, TypeHue},
// 		{&GenericLight{}, TypeGeneric},
// 	}
// 	for _, tt := range tests {
// 		assert.Equal(t, tt.expected, tt.input.GetType())
// 	}
// }

func TestDuplicateFixtures(t *testing.T) {
	c := config.HaloConfig{
		PatchedFixtures: []config.PatchedFixture{
			config.PatchedFixture{
				Name: "fixture1",
			},
			config.PatchedFixture{
				Name: "fixture1",
			},
		},
	}

	_, err := NewManager(c)
	require.Error(t, err)
}
