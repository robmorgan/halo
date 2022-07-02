package profile

const (
	ChannelTypeIntensity = "channel:type:intensity"
	ChannelTypeRed       = "channel:type:red"
	ChannelTypeGreen     = "channel:type:green"
	ChannelTypeBlue      = "channel:type:blue"
	ChannelTypeWhite     = "channel:type:white"

	ChannelTypeMotorPosition = "channel:type:motor:position"
	ChannelTypeMotorSpeed    = "channel:type:motor:speed"

	ChannelTypeFunctionSelect = "channel:type:function:select"
	ChannelTypeFunctionSpeed  = "channel:type:function:speed"
)

// Profile holds info for a fixture profile including the channel and capability mappings.
type Profile struct {
	Name         string
	Capabilities []string

	// The fixture channels
	Channels map[string]int
}
