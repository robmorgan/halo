package main

import "github.com/robmorgan/halo/fixture"

// TODO - you probably want some sort of convience method to access all fixtures for updates
type PatchedFixtures struct {
	// Root fixture group that contains all patched fixtures
	Root *fixture.Group

	// Custom Groups

	// FrontPars stores all the front-facing PARs in a fixture group
	FrontPars *fixture.Group

	// UplightPars stores all of the uplight PARs in a fixture group
	UplightPars *fixture.Group
}

func PatchFixtures() *PatchedFixtures {

	rootFg := fixture.NewGroup()
	frontPars := patchFrontPars()
	uplightPars := patchUplightPars()

	// make sure everything is available on the root fixture group
	rootFg.Merge(
		frontPars,
		uplightPars,
	)

	// patch all the fixtures
	patchedFixtures := PatchedFixtures{
		Root:        rootFg,
		FrontPars:   patchFrontPars(),
		UplightPars: patchUplightPars(),
	}

	return &patchedFixtures
}

func patchFrontPars() *fixture.Group {
	fg := fixture.NewGroup()

	// left middle par
	// For some reason the DMX address we are using at the moment is really -1 to what the actual
	// fixture address is
	par1 := fixture.NewFixture(1, 114, 8, map[int]*fixture.Channel{
		1: {
			Type:       fixture.TypeIntensity,
			Address:    1,
			Resolution: 1,
		},
		2: {
			Type:       fixture.TypeColorRed,
			Address:    2,
			Resolution: 1,
		},
	})
	fg.AddFixture("left_middle_par", par1)

	// right middle par
	par2 := fixture.NewFixture(2, 138, 8, map[int]*fixture.Channel{
		1: {
			Type:       fixture.TypeIntensity,
			Address:    1,
			Resolution: 1,
		},
		2: {
			Type:       fixture.TypeColorRed,
			Address:    2,
			Resolution: 1,
		},
	})
	fg.AddFixture("right_middle_par", par2)

	// TODO - remove. cheat a bit by setting intensity.
	par1.SetIntensity(1.0)
	par2.SetIntensity(1.0)

	return fg
}

func patchUplightPars() *fixture.Group {
	fg := fixture.NewGroup()

	// left uplight par (A.123 -> 122)
	par3 := fixture.NewFixture(3, 122, 8, map[int]*fixture.Channel{
		1: {
			Type:       fixture.TypeIntensity,
			Address:    1,
			Resolution: 1,
		},
		2: {
			Type:       fixture.TypeColorRed,
			Address:    2,
			Resolution: 1,
		},
	})
	fg.AddFixture("left_uplight_par", par3)

	// right uplight par (A.131 -> 130)
	par4 := fixture.NewFixture(3, 130, 8, map[int]*fixture.Channel{
		1: {
			Type:       fixture.TypeIntensity,
			Address:    1,
			Resolution: 1,
		},
		2: {
			Type:       fixture.TypeColorRed,
			Address:    2,
			Resolution: 1,
		},
	})
	fg.AddFixture("right_uplight_par", par4)

	// TODO - remove. cheat a bit by setting intensity.
	par3.SetIntensity(1.0)
	par4.SetIntensity(1.0)

	return fg
}
