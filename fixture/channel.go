package fixture

// Channel represents a channel on the fixture
type Channel struct {
	Type       string
	Address    int
	Resolution int

	// Halo stores all fixture values as float64 so the value can be between 0 and 1.
	Value float64
}
