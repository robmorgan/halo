package dmx

import "encoding/hex"

type Address int
type Channel uint8

const UniverseChannels Address = 512

// Universe represents a DMX universe
type Universe struct {
	Channels []Channel
}

// NewUniverse creates a new DMX universe.
func NewUniverse() *Universe {
	return &Universe{
		Channels: make([]Channel, 0, UniverseChannels),
	}
}

func (u *Universe) Bytes() []byte {
	var buf = make([]byte, len(u.Channels))
	for i, channel := range u.Channels {
		buf[i] = byte(channel)
	}
	return buf
}

func (u *Universe) String() string {
	return hex.Dump(u.Bytes())
}

func (u *Universe) Get(address Address) Channel {
	if address <= 0 || address > UniverseChannels {
		panic("Invalid DMX address")
	} else if int(address) > len(u.Channels) {
		return 0
	}

	return u.Channels[address-1]
}

func (u *Universe) Set(address Address, value Channel) {
	if address <= 0 || address > UniverseChannels {
		panic("Invalid DMX address")
	}
	// } else if int(address) > len(u) {
	// 	// TODO - what is this doing?
	// 	*universe = (*universe)[0:address]
	// 	u.Channels = u.Channels[0:address]
	// }

	u.Channels = append(u.Channels[:address+1], u.Channels[address:]...) // index < len(a)
	u.Channels[address] = value

	//	*u.Channels[address] = value
}

type Writer interface {
	WriteDMX(dmx Universe) error
}
