package config

// PatchedFixture stores config info for a dmx fixture
type PatchedFixture struct {
	Name     string
	Address  int
	Universe int
	Profile  string
}

func PatchFixtures() []PatchedFixture {
	s := make([]PatchedFixture, 0)

	s = append(s, patchFrontPars()...)
	s = append(s, patchUplightPars()...)

	return s
}

func patchFrontPars() []PatchedFixture {
	return []PatchedFixture{
		// left middle par
		PatchedFixture{
			Name:     "left_middle_par",
			Address:  114,
			Universe: 1,
			Profile:  "shehds-par",
		},
		// right middle par
		PatchedFixture{
			Name:     "right_middle_par",
			Address:  138,
			Universe: 1,
			Profile:  "shehds-par",
		},
	}
}

func patchUplightPars() []PatchedFixture {
	return []PatchedFixture{
		// left uplight par (A.123 -> 122)
		PatchedFixture{
			Name:     "left_uplight_par",
			Address:  122,
			Universe: 1,
			Profile:  "shehds-par",
		},
		// right uplight par (A.131 -> 130)
		PatchedFixture{
			Name:     "right_uplight_par",
			Address:  130,
			Universe: 1,
			Profile:  "shehds-par",
		},
	}
}
