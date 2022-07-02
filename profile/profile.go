package profile

const (
	ChannelTypeIntensity = "channel:type:intensity"
	ChannelTypeRed       = "channel:type:red"
	ChannelTypeGreen     = "channel:type:green"
	ChannelTypeBlue      = "channel:type:blue"
)

// Profile holds info for a fixture profile including the channel and capability mappings.
type Profile struct {
	Name         string
	Capabilities []string

	// The fixture channels
	Channels map[string]int
}
