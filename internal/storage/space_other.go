//go:build !linux && !darwin

package storage

func measureSpace(string) (Space, error) { return Space{}, ErrMeasurement }
