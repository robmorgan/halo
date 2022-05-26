package fixture

import (
	"fmt"
)

type Group struct {
	Fixtures map[string]*Fixture
}

// Create a new FixtureGroup object with reasonable defaults for real usage.
func NewGroup() *Group {
	return &Group{
		Fixtures: make(map[string]*Fixture),
	}
}

func (fg *Group) GetFixture(id string) (*Fixture, error) {
	if fixture, found := fg.Fixtures[id]; found {
		return fixture, nil
	} else {
		return nil, fmt.Errorf("the fixture group does not contain a fixture with the id: %s", id)
	}
}

func (fg *Group) SetFixtures(fixtures map[string]*Fixture) {
	fg.Fixtures = fixtures
}

func (fg *Group) AddFixture(id string, fixture *Fixture) {
	fg.Fixtures[id] = fixture
}

// HasFixtures returns true if there are fixtures in the group
func (fg *Group) HasFixtures() bool {
	return len(fg.Fixtures) > 0
}

// Count returns the number of fixtures in the group
func (fg *Group) Count() int {
	return len(fg.Fixtures)
}
