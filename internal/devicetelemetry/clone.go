package devicetelemetry

func copyValue[T any](value *T) *T {
	if value == nil {
		return nil
	}
	copy := *value
	return &copy
}
func copyLinks(links []WirelessLink) []WirelessLink {
	result := append([]WirelessLink{}, links...)
	for i := range result {
		result[i].MLD = copyValue(result[i].MLD)
		result[i].SignalDBM = copyValue(result[i].SignalDBM)
		result[i].NoiseDBM = copyValue(result[i].NoiseDBM)
		result[i].AgeingSeconds = copyValue(result[i].AgeingSeconds)
	}
	return result
}
func copyObservation(o Observation) Observation {
	o.Counters = append([]Counter{}, o.Counters...)
	o.Links = copyLinks(o.Links)
	o.OnlineSeconds = copyValue(o.OnlineSeconds)
	o.AgeingSeconds = copyValue(o.AgeingSeconds)
	return o
}

// CloneSnapshot prevents callers from changing cached source or baseline data.
func CloneSnapshot(s Snapshot) Snapshot {
	s.Devices = append([]Observation{}, s.Devices...)
	for i := range s.Devices {
		s.Devices[i] = copyObservation(s.Devices[i])
	}
	return s
}
