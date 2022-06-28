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

// HasFixture returns true if the group contains the specified fixture
func (fg *Group) HasFixture(id string) bool {
	if _, found := fg.Fixtures[id]; found {
		return true
	}
	return false
}

// HasFixtures returns true if there are fixtures in the group
func (fg *Group) HasFixtures() bool {
	return len(fg.Fixtures) > 0
}

// Merge the specified fixture groups into this one and return it
func (fg *Group) Merge(groups ...*Group) *Group {
	out := fg.Fixtures

	for _, group := range groups {
		// The fixture group only stores fixtures at the moment, so this is all we need to copy.
		for key, value := range group.Fixtures {
			out[key] = value
		}
	}

	fg.Fixtures = out
	return fg
}

// Count returns the number of fixtures in the group
func (fg *Group) Count() int {
	return len(fg.Fixtures)
}
