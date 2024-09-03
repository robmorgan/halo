package main

var playlists = []Playlist{
	{
		ID:   3,
		Name: "MiddlePAR-Cycle130-Gold",
	},
	{
		ID:   4,
		Name: "PAR-Strobe-Fast-White",
	},
}

type Playlist struct {
	ID   int
	Name string
}

func getPlaylists() []Playlist {
	p := playlists
	copy(p, playlists)
	return p
}
