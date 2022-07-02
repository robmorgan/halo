package utils

import (
	"fmt"
	"strings"

	"github.com/aybabtme/rgbterm"
	colorful "github.com/lucasb-eyer/go-colorful"

	"github.com/robmorgan/halo/logger"
)

//RGB holds RGB values (0-255, although they can be negative in the case of a delta)
type RGB struct {
	R int `json:"r"`
	G int `json:"g"`
	B int `json:"b"`
}

//IsBlack determines if a color is black
func (c *RGB) IsBlack() bool {
	return *c == GetRGBFromString("black")
}

//GetInterpolatedFade returns fade from one color to another.
func (c *RGB) GetInterpolatedFade(target RGB, step, numSteps int) RGB {
	c1 := c.AsColorful()
	c2 := target.AsColorful()
	progress := float64(step) / float64(numSteps-1)
	if progress == 1 {
		return target
	}

	return GetRGBFromColorful(c1.BlendHcl(c2, progress).Clamped())
}

//AsColorful turns a colorful.Color into an RGB
func (c *RGB) AsColorful() colorful.Color {
	return colorful.Color{R: float64(c.R) / 255, G: float64(c.G) / 255, B: float64(c.B) / 255}
}

//GetRGBFromColorful turns a colorful.Color struct into an RGB one
func GetRGBFromColorful(c colorful.Color) RGB {
	return RGB{
		R: int(c.R * 255),
		G: int(c.G * 255),
		B: int(c.B * 255),
	}
}

//ToHex converts a color to hex
func (c *RGB) ToHex() string {
	return c.AsColorful().Hex()
}

//GetRGBFromString turns a string into an RGB color
//The input can either be hex (#00FF00) or a string (green)
func GetRGBFromString(s string) RGB {
	if strings.HasPrefix(s, "#") {
		//parse hex
		c, err := colorful.Hex(s)
		if err != nil {
			logger := logger.GetProjectLogger()
			logger.Errorf("error getting RGB from string: %s, %v", s, err)
			return RGB{}
		}
		return GetRGBFromColorful(c)
	}

	//fallback to string matching
	switch s {
	case "red":
		return RGB{R: 255}
	case "green":
		return RGB{G: 255}
	case "blue":
		return RGB{B: 255}
	case "white":
		return RGB{R: 255, G: 255, B: 255}
	case "black":
		return RGB{R: 0, G: 0, B: 0}
	default:
		return RGB{}
	}

}

//AsComponents returns the seperate r, g, b
func (c *RGB) AsComponents() (int, int, int) {
	return c.R, c.G, c.B
}

//TermString returns a ANSI-color formatted r/g/b string
func (c *RGB) TermString() string {
	rgbstr := fmt.Sprintf("%d,%d,%d", c.R, c.G, c.B)
	return string(rgbterm.Bytes([]byte("█"+rgbstr+"█"), uint8(c.R), uint8(c.G), uint8(c.B), 0, 0, 0))
}

//GetXyy returns the RGB color in xyy color space
func (c *RGB) GetXyy() (x, y, Yout float64) {
	//OLD:  x, y, _ := colorful.Xyz()
	return colorful.XyzToXyy(colorful.LinearRgbToXyz(c.AsColorful().LinearRgb()))
}
