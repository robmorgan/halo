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
	fmt.Printf("setting ch value to: %.7f\n", value)
	c.Value = value
	fmt.Printf("resettt ch value to: %.7f\n", c.Value)
	fmt.Printf("checj ch value to: %.7f\n", c.GetValue())
}

func (c *Channel) GetValue() float64 {
	return c.Value
}

// func (c *Channel) toDMX() dmx.Channel {
// 	var val dmx.Channel
// 	val = (dmx.Channel)(uint8(c.Value) * 255)
// 	return val
// }
