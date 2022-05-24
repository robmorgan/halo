package fixture

type FixtureGroup struct {
	Fixtures []*Fixture
}

// HasFixtures returns true if there are fixtures in the group
func (fg *FixtureGroup) HasFixtures() bool {
	return len(fg.Fixtures) > 0
}

// Count returns the number of fixtures in the group
func (fg *FixtureGroup) Count() int {
	return len(fg.Fixtures)
}
