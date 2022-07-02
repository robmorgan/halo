package fixture

import (
	"context"
	"fmt"
	"log"
	"sync"
	"time"
)

// DMXState holds the DMX512 values for each channel
type DMXState struct {
	universes map[int][]byte
	lock      sync.Mutex
}

type dmxOperation struct {
	universe, channel, value int
}

func (s *DMXState) getValue(universe, channel int) int {
	return int(s.universes[universe][channel-1])
}

func (s *DMXState) set(ops ...dmxOperation) error {
	s.lock.Lock()
	defer s.lock.Unlock()
	for _, op := range ops {
		channel := op.channel
		universe := op.universe
		value := op.value
		if channel < 1 || channel > 255 {
			return fmt.Errorf("dmx channel (%d) not in range, op=%v", channel, op)
		}

		s.initializeUniverse(universe)
		s.universes[universe][channel-1] = byte(value)
	}

	return nil
}

func (s *DMXState) initializeUniverse(universe int) {
	if s.universes[universe] == nil {
		chans := make([]byte, 255)
		s.universes[universe] = chans
	}
}

// GetDMXState returns the current dmx state
func (m *StateManager) GetDMXState() *DMXState {
	return &m.dmxState
}

// SetDMXState updates the dmxstate
func (m *StateManager) SetDMXState(ops ...dmxOperation) error {
	return m.dmxState.set(ops...)
}

// OLAClient is the interface for communicating with OLA
type OLAClient interface {
	SendDmx(universe int, values []byte) (status bool, err error)
	Close()
}

// SendDMXWorker sends OLA the current dmxState across all universes
func SendDMXWorker(ctx context.Context, client OLAClient, tick time.Duration, manager Manager, wg *sync.WaitGroup) error {
	defer wg.Done()
	defer client.Close()

	t := time.NewTimer(tick)
	defer t.Stop()
	log.Printf("timer started at %v", time.Now())

	for {
		select {
		case <-ctx.Done():
			log.Println("SendDMXWorker shutdown")
			return ctx.Err()
		case <-t.C:
			for k, v := range manager.GetDMXState().universes {
				client.SendDmx(k, v)
			}
			t.Reset(tick)
		}
	}
}
