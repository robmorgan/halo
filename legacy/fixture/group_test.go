package fixture

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestFixture(t *testing.T) {
	t.Parallel()

	fix := NewFixture(1, 138, 8, "test")
	fg1 := NewGroup()
	fg1.AddFixture("fix1", fix)

	// set a value
	fix1, err := fg1.GetFixture("fix1")
	require.NoError(t, err)
	fix1.SetIntensity(0.65)

	require.NoError(t, err)
	require.Equal(t, 0.65, fix1.GetIntensity())
}

func TestFixtureInMultipleGroups(t *testing.T) {
	t.Parallel()

	fix := NewFixture(1, 138, 8, "test")

	// add the fixture to two fixture groups
	fg1 := NewGroup()
	fg2 := NewGroup()
	fg1.AddFixture("fix1", fix)
	fg2.AddFixture("left_par", fix)

	// set a value
	fix1, err := fg1.GetFixture("fix1")
	require.NoError(t, err)
	fix1.SetIntensity(0.65)

	// check its correct in the other fixture group
	par, err := fg1.GetFixture("fix1")
	require.NoError(t, err)
	require.Equal(t, 0.65, par.GetIntensity())
}

func TestMerge(t *testing.T) {
	t.Parallel()

	fix1 := NewFixture(1, 1, 8, "")
	fix2 := NewFixture(2, 10, 8, "")
	fix3 := NewFixture(3, 20, 8, "")

	// add the fixtures to three separate groups
	fg1 := NewGroup()
	fg2 := NewGroup()
	fg3 := NewGroup()
	fg1.AddFixture("fix1", fix1)
	fg2.AddFixture("fix2", fix2)
	fg3.AddFixture("fix2", fix3) // name collision (will replace)

	// set some values
	fix1.SetIntensity(0.3)
	fix2.SetIntensity(0.6)
	fix3.SetIntensity(0.8)

	// merge them over the first group
	fg := fg1.Merge(fg2, fg3)

	// check everything is correct
	require.True(t, fg.HasFixture("fix1"))
	require.True(t, fg.HasFixture("fix2"))

	fix, err := fg.GetFixture("fix2")
	require.NoError(t, err)
	require.Equal(t, 3, fix.Id)
	require.Equal(t, 20, fix.Address)
	require.Equal(t, 0.8, fix.GetIntensity())
	require.True(t, fix.NeedsUpdate())
}
