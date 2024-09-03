package fixture

import (
	"fmt"
	"sync"

	"github.com/robmorgan/halo/config"
)

// Manager is the fixture manager interface
type Manager interface {
	SetState(name string, new State)
	GetState(name string) *State
	GetFixtureNames() []string
	GetAllStates() *StateMap
	GetByName(name string) Interface
	GetFixturesByName() NameMap
	GetDMXState() *DMXState
	SetDMXState(ops ...dmxOperation) error
}

// NameMap holds string-keyed Lights
type NameMap map[string]Interface

// StateMap holds global fixture state
type StateMap map[string]State

// StateManager holds the state of fixtures
type StateManager struct {
	states    StateMap
	items     NameMap
	stateLock sync.RWMutex
	dmxState  DMXState
}

// SetState will set the current state for a light
func (m *StateManager) SetState(name string, new State) {
	m.stateLock.Lock()
	defer m.stateLock.Unlock()
	m.states[name] = new
}

// GetState will get the current state for a light
func (m *StateManager) GetState(name string) *State {
	m.stateLock.RLock()
	defer m.stateLock.RUnlock()
	state, ok := m.states[name]
	if ok {
		return &state
	}
	return nil
}

// GetLightNames returns all the light names
func (m *StateManager) GetFixtureNames() []string {
	keys := make([]string, 0, len(m.items))
	for k := range m.items {
		keys = append(keys, k)
	}
	return keys
}

// GetAllStates will get the current state for all lights
func (m *StateManager) GetAllStates() *StateMap {
	return &m.states
}

// GetFixturesByName returns lights keyed by name
func (m *StateManager) GetFixturesByName() NameMap {
	return m.items
}

// GetByName looks up a fixture by name
func (m *StateManager) GetByName(name string) Interface {
	fixture, ok := m.items[name]
	if ok {
		return fixture
	}
	return nil
}

// NewManager parses fixture config
func NewManager(config config.HaloConfig) (Manager, error) {
	m := StateManager{
		states:   make(StateMap),
		items:    make(NameMap),
		dmxState: DMXState{universes: make(map[int][]byte)},
	}

	// get all the available fixtures
	for i := range config.PatchedFixtures {
		x := &config.PatchedFixtures[i]

		if _, ok := m.items[x.Name]; ok {
			err := fmt.Errorf("duplicate fixtures found! name=%s", x.Name)
			return nil, err
		}
		m.items[x.Name] = &Fixture{
			Name:     x.Name,
			Address:  x.Address,
			Universe: x.Universe,
			Profile:  x.Profile,
		}
		m.SetState(x.Name, State{})
	}

	return &m, nil
}
