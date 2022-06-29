package fixture

const (
	TypeIntensity  = "type:intensity"
	TypeColorRed   = "type:color:red"
	TypeColorGreen = "type:color:green"
	TypeColorBlue  = "type:color:blue"
)

// Channel represents a channel on the fixture
type Channel struct {
	Type       string
	Address    int
	Resolution int
}

// func (c *Channel) toDMX() dmx.Channel {
// 	var val dmx.Channel
// 	val = (dmx.Channel)(uint8(c.Value) * 255)
// 	return val
// }
