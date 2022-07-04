package config

import "github.com/robmorgan/halo/profile"

func initializeFixtureProfiles() map[string]profile.Profile {
	out := map[string]profile.Profile{
		"shehds-par": {
			Name: "Shehds LED Flat PAR 12x3W RGBW",
			Channels: map[string]int{
				profile.ChannelTypeIntensity:      1,
				profile.ChannelTypeRed:            2,
				profile.ChannelTypeGreen:          3,
				profile.ChannelTypeBlue:           4,
				profile.ChannelTypeWhite:          5,
				profile.ChannelTypeStrobe:         6,
				profile.ChannelTypeFunctionSelect: 7,
				profile.ChannelTypeUnknown:        8,
			},
		},
		"shehds-led-spot-60w": {
			Name: "Shehds LED Spot 60W",
			// 10 channel mode
			Channels: map[string]int{
				profile.ChannelTypePan:            1,
				profile.ChannelTypeTilt:           2,
				profile.ChannelTypeColor:          3,
				profile.ChannelTypeGobo:           4,
				profile.ChannelTypeStrobe:         5,
				profile.ChannelTypeIntensity:      6,
				profile.ChannelTypeMotorSpeed:     7,
				profile.ChannelTypeFunctionSelect: 8,
				profile.ChannelTypeReset:          9,
			},
		},
		"shehds-led-wash-7x18w-rgbwa-uv": {
			Name: "Shehds LED Wash 7x18W RGBWA+UV",
			// 10 channel mode
			Channels: map[string]int{
				profile.ChannelTypePan:       1,
				profile.ChannelTypeTilt:      2,
				profile.ChannelTypeIntensity: 3,
				profile.ChannelTypeRed:       4,
				profile.ChannelTypeGreen:     5,
				profile.ChannelTypeBlue:      6,
				profile.ChannelTypeWhite:     7,
				profile.ChannelTypeAmber:     8,
				profile.ChannelTypeUV:        9,
				profile.ChannelTypeUnknown:   10, // TODO - I think this is XY speed?  Check the manual
			},
		},
		"shehds-led-bar-beam-8x12w": {
			Name: "Shehds LED Bar Beam 8x12W RGBW",
			// 38 channel mode
			Channels: map[string]int{
				profile.ChannelTypeTilt:           1,
				profile.ChannelTypeTiltSpeed:      2,
				profile.ChannelTypeFunctionSelect: 3,
				profile.ChannelTypeFunctionSpeed:  4,
				profile.ChannelTypeIntensity:      5,
				profile.ChannelTypeRed:            6,
				profile.ChannelTypeGreen:          7,
				profile.ChannelTypeBlue:           8,
				profile.ChannelTypeWhite:          9,
			},
		},
		"shehds-led-bar-beam-8x12w-38ch": {
			Name: "Shehds LED Bar Beam 8x12W RGBW",
			// 38 channel mode
			Channels: map[string]int{
				profile.ChannelTypeTilt:           1,
				profile.ChannelTypeTiltSpeed:      2,
				profile.ChannelTypeFunctionSelect: 3,
				profile.ChannelTypeFunctionSpeed:  4,
				profile.ChannelTypeIntensity:      5,
				profile.ChannelTypeStrobe:         6,

				// light 1
				profile.ChannelTypeRed + "1":   7,
				profile.ChannelTypeGreen + "1": 8,
				profile.ChannelTypeBlue + "1":  9,
				profile.ChannelTypeWhite + "1": 10,

				// light 2
				profile.ChannelTypeRed + "2":   11,
				profile.ChannelTypeGreen + "2": 12,
				profile.ChannelTypeBlue + "2":  13,
				profile.ChannelTypeWhite + "2": 14,

				// light 3
				profile.ChannelTypeRed + "3":   15,
				profile.ChannelTypeGreen + "3": 16,
				profile.ChannelTypeBlue + "3":  17,
				profile.ChannelTypeWhite + "3": 18,

				// light 4
				profile.ChannelTypeRed + "4":   19,
				profile.ChannelTypeGreen + "4": 20,
				profile.ChannelTypeBlue + "4":  21,
				profile.ChannelTypeWhite + "4": 22,

				// light 5
				profile.ChannelTypeRed + "5":   23,
				profile.ChannelTypeGreen + "5": 24,
				profile.ChannelTypeBlue + "5":  25,
				profile.ChannelTypeWhite + "5": 26,

				// light 6
				profile.ChannelTypeRed + "6":   27,
				profile.ChannelTypeGreen + "6": 28,
				profile.ChannelTypeBlue + "6":  29,
				profile.ChannelTypeWhite + "6": 30,

				// light 7
				profile.ChannelTypeRed + "7":   31,
				profile.ChannelTypeGreen + "7": 32,
				profile.ChannelTypeBlue + "7":  33,
				profile.ChannelTypeWhite + "7": 34,

				// light 8
				profile.ChannelTypeRed + "8":   35,
				profile.ChannelTypeGreen + "8": 36,
				profile.ChannelTypeBlue + "8":  37,
				profile.ChannelTypeWhite + "8": 38,
			},
		},
	}

	return out
}
