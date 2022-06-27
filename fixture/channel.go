package fixture

import "fmt"

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

	// Halo stores all fixture values as float64 so the value can be between 0 and 1.
	Value float64
}

func (c *Channel) SetValue(value float64) {
	c.Value = value
}

func (c *Channel) GetValue() float64 {
	return c.Value
}

// func (c *Channel) toDMX() dmx.Channel {
// 	var val dmx.Channel
// 	val = (dmx.Channel)(uint8(c.Value) * 255)
// 	return val
// }
