package config

import "github.com/robmorgan/halo/profile"

func initializeFixtureProfiles() map[string]profile.Profile {
	out := map[string]profile.Profile{
		"shehds-par": {
			Name: "Shehds LED Flat PAR 12x3W RGBW",
			Channels: map[string]int{
				profile.ChannelTypeIntensity: 1,
				profile.ChannelTypeRed:       2,
				profile.ChannelTypeGreen:     3,
				profile.ChannelTypeBlue:      4,
			},
		},
		"shehds-led-bar-beam-8x12w": {
			Name: "Shehds LED Bar Beam 8x12W RGBW",
			// 9 channel mode
			Channels: map[string]int{
				profile.ChannelTypeMotorPosition:  1,
				profile.ChannelTypeMotorSpeed:     2,
				profile.ChannelTypeFunctionSelect: 3,
				profile.ChannelTypeFunctionSpeed:  4,
				profile.ChannelTypeIntensity:      5,
				profile.ChannelTypeRed:            6,
				profile.ChannelTypeGreen:          7,
				profile.ChannelTypeBlue:           8,
				profile.ChannelTypeWhite:          9,
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
	}

	return out
}
