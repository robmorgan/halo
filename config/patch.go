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

	s = append(s, patchFrontMiddlePars()...)
	s = append(s, patchFrontTopPars()...)
	s = append(s, patchUplightPars()...)
	s = append(s, patchBeamBars()...)
	s = append(s, patchSpotLights()...)
	s = append(s, patchWashLights()...)

	return s
}

func patchFrontMiddlePars() []PatchedFixture {
	return []PatchedFixture{
		// left middle par
		PatchedFixture{
			Name:     "left_middle_par",
			Address:  1,
			Universe: 1,
			Profile:  "shehds-par",
		},
		// right middle par
		PatchedFixture{
			Name:     "right_middle_par",
			Address:  9,
			Universe: 1,
			Profile:  "shehds-par",
		},
	}
}

func patchFrontTopPars() []PatchedFixture {
	return []PatchedFixture{
		// left top par
		PatchedFixture{
			Name:     "left_top_par",
			Address:  17,
			Universe: 1,
			Profile:  "shehds-par",
		},
		// right top par
		PatchedFixture{
			Name:     "right_top_par",
			Address:  25,
			Universe: 1,
			Profile:  "shehds-par",
		},
	}
}

func patchUplightPars() []PatchedFixture {
	return []PatchedFixture{
		PatchedFixture{
			Name:     "left_uplight_par",
			Address:  33,
			Universe: 1,
			Profile:  "shehds-par",
		},
		PatchedFixture{
			Name:     "right_uplight_par",
			Address:  41,
			Universe: 1,
			Profile:  "shehds-par",
		},
	}
}

func patchBeamBars() []PatchedFixture {
	return []PatchedFixture{
		PatchedFixture{
			Name:     "left_beam_bar",
			Address:  105,
			Universe: 1,
			Profile:  "shehds-led-bar-beam-8x12w",
		},
		PatchedFixture{
			Name:     "right_beam_bar",
			Address:  114,
			Universe: 1,
			Profile:  "shehds-led-bar-beam-8x12w",
		},
	}
}

func patchSpotLights() []PatchedFixture {
	return []PatchedFixture{
		PatchedFixture{
			Name:     "left_spot",
			Address:  137,
			Universe: 1,
			Profile:  "shehds-led-spot-60w",
		},
		PatchedFixture{
			Name:     "right_spot",
			Address:  137,
			Universe: 1,
			Profile:  "shehds-led-spot-60w",
		},
	}
}

func patchWashLights() []PatchedFixture {
	return []PatchedFixture{
		PatchedFixture{
			Name:     "left_wash",
			Address:  55,
			Universe: 1,
			Profile:  "shehds-led-wash-7x18w-rgbwa-uv",
		},
		PatchedFixture{
			Name:     "right_wash",
			Address:  65,
			Universe: 1,
			Profile:  "shehds-led-wash-7x18w-rgbwa-uv",
		},
	}
}
