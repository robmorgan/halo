package fixture

const (
	TypeIntensity  = "type:intensity"
	TypeColorRed   = "type:color:red"
	TypeColorGreen = "type:color:green"
	TypeColorBlue  = "type:color:blue"
)

// Profile holds info for a fixture profile including the channel and capability mappings.
type Profile struct {
	Name         string
	Capabilities []string

	// The fixture channels
	Channels map[int]string
}
