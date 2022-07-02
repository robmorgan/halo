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
	s = append(s, patchBeamBars()...)

	return s
}

func patchFrontPars() []PatchedFixture {
	return []PatchedFixture{
		// left middle par
		PatchedFixture{
			Name:     "left_middle_par",
			Address:  115,
			Universe: 1,
			Profile:  "shehds-par",
		},
		// right middle par
		PatchedFixture{
			Name:     "right_middle_par",
			Address:  139,
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

func patchBeamBars() []PatchedFixture {
	return []PatchedFixture{
		PatchedFixture{
			Name:     "left_beam_bar",
			Address:  163,
			Universe: 1,
			Profile:  "shehds-led-bar-beam-8x12w",
		},
		PatchedFixture{
			Name:     "right_beam_bar",
			Address:  57,
			Universe: 1,
			Profile:  "shehds-led-bar-beam-8x12w",
		},
	}
}
