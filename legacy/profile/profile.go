package profile

const (
	ChannelTypeIntensity = "channel:type:intensity"
	ChannelTypeStrobe    = "channel:type:strobe"

	ChannelTypeRed   = "channel:type:red"
	ChannelTypeGreen = "channel:type:green"
	ChannelTypeBlue  = "channel:type:blue"
	ChannelTypeWhite = "channel:type:white"
	ChannelTypeAmber = "channel:type:amber"
	ChannelTypeUV    = "channel:type:uv"
	ChannelTypeColor = "channel:type:color" // Generic color channel (Shehds spots)

	ChannelTypePan       = "channel:type:pan"
	ChannelTypePanSpeed  = "channel:type:panspeed"
	ChannelTypeTilt      = "channel:type:tilt"
	ChannelTypeTiltSpeed = "channel:type:tiltspeed"

	ChannelTypeGobo = "channel:type:gobo"

	ChannelTypeMotorPosition = "channel:type:motor:position"
	ChannelTypeMotorSpeed    = "channel:type:motor:speed"

	ChannelTypeFunctionSelect = "channel:type:function:select"
	ChannelTypeFunctionSpeed  = "channel:type:function:speed"

	ChannelTypeReset   = "channel:type:reset"
	ChannelTypeUnknown = "channel:type:unknown"
)

// Profile holds info for a fixture profile including the channel and capability mappings.
type Profile struct {
	Name         string
	Capabilities []string

	// The fixture channels
	Channels map[string]int
}
