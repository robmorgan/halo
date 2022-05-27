package fixture

import "github.com/robmorgan/halo/dmx"

const (
	TypeIntensity  = "type:intensity"
	TypeColorRed   = "type:color:red"
	TypeColorGreen = "type:color:green"
	TypeColorBlue  = "type:color:blue"
)

type Value float64

// Channel represents a channel on the fixture
type Channel struct {
	Type       string
	Address    int
	Resolution int

	// Halo stores all fixture values as float64 so the value can be between 0 and 1.
	Value Value
}

func (c *Channel) SetValue(value Value) {
	c.Value = value
}

func (c *Channel) toDMX() dmx.Channel {
	var val dmx.Channel
	val = (dmx.Channel)(uint8(c.Value) * 255)
	return val
}
