package main

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/binary"
	"errors"
	"io"
	"net"
	"strings"
	"time"
)

const dnsReadinessMaxPacket = 4096

// probeDNSReadiness proves that the selected local hijack-dns path returns an
// actual answer, not merely that a TCP socket accepts connections. Each lookup
// is bounded independently and is also interrupted by the startup context.
func probeDNSReadiness(ctx context.Context, network, address, domain string, timeout time.Duration) error {
	if network != "udp" && network != "tcp" {
		return errors.New("invalid DNS readiness network")
	}
	if timeout <= 0 {
		return errors.New("invalid DNS readiness timeout")
	}
	var transaction [2]byte
	if _, err := rand.Read(transaction[:]); err != nil {
		return err
	}
	query, err := dnsReadinessQuery(domain, binary.BigEndian.Uint16(transaction[:]))
	if err != nil {
		return err
	}
	probeCtx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()
	dialer := net.Dialer{}
	conn, err := dialer.DialContext(probeCtx, network, address)
	if err != nil {
		return err
	}
	defer conn.Close()
	deadline, _ := probeCtx.Deadline()
	if err := conn.SetDeadline(deadline); err != nil {
		return err
	}
	stop := context.AfterFunc(probeCtx, func() { conn.Close() })
	defer stop()
	var response []byte
	if network == "udp" {
		if n, err := conn.Write(query); err != nil {
			return err
		} else if n != len(query) {
			return io.ErrShortWrite
		}
		response = make([]byte, dnsReadinessMaxPacket+1)
		n, err := conn.Read(response)
		if err != nil {
			return err
		}
		if n > dnsReadinessMaxPacket {
			return errors.New("DNS readiness response exceeds limit")
		}
		response = response[:n]
	} else {
		packet := make([]byte, len(query)+2)
		binary.BigEndian.PutUint16(packet[:2], uint16(len(query)))
		copy(packet[2:], query)
		if _, err := io.Copy(conn, bytes.NewReader(packet)); err != nil {
			return err
		}
		var prefix [2]byte
		if _, err := io.ReadFull(conn, prefix[:]); err != nil {
			return err
		}
		size := int(binary.BigEndian.Uint16(prefix[:]))
		if size < 12 || size > dnsReadinessMaxPacket {
			return errors.New("DNS readiness response size invalid")
		}
		response = make([]byte, size)
		if _, err := io.ReadFull(conn, response); err != nil {
			return err
		}
	}
	if err := probeCtx.Err(); err != nil {
		return err
	}
	return validateDNSReadinessResponse(response, query)
}

func dnsReadinessQuery(domain string, transaction uint16) ([]byte, error) {
	domain = normalizedDNSReadinessDomain(domain)
	if domain == "" {
		return nil, errors.New("DNS readiness query domain invalid")
	}
	query := make([]byte, 12, len(domain)+18)
	binary.BigEndian.PutUint16(query[:2], transaction)
	binary.BigEndian.PutUint16(query[2:4], 0x0100) // Recursion desired.
	binary.BigEndian.PutUint16(query[4:6], 1)
	for _, label := range strings.Split(domain, ".") {
		query = append(query, byte(len(label)))
		query = append(query, label...)
	}
	return append(query, 0, 0, 1, 0, 1), nil // Root terminator, A, IN.
}

func validateDNSReadinessResponse(response, query []byte) error {
	if len(query) < 12 || len(response) < 12 || len(response) > dnsReadinessMaxPacket {
		return errors.New("DNS readiness response malformed")
	}
	if binary.BigEndian.Uint16(response[:2]) != binary.BigEndian.Uint16(query[:2]) {
		return errors.New("DNS readiness transaction mismatch")
	}
	flags := binary.BigEndian.Uint16(response[2:4])
	if flags&0x8000 == 0 || flags&0x7800 != 0 || flags&0x0200 != 0 || flags&0x0040 != 0 || flags&0x000f != 0 {
		return errors.New("DNS readiness response flags or rcode invalid")
	}
	if binary.BigEndian.Uint16(response[4:6]) != 1 {
		return errors.New("DNS readiness response question count invalid")
	}
	requested, queryEnd, err := dnsReadinessName(query, 12)
	if err != nil || queryEnd+4 != len(query) {
		return errors.New("DNS readiness query malformed")
	}
	question, offset, err := dnsReadinessName(response, 12)
	if err != nil || offset+4 > len(response) || question != requested || !bytes.Equal(response[offset:offset+4], query[queryEnd:queryEnd+4]) {
		return errors.New("DNS readiness response question mismatch")
	}
	offset += 4
	aliases := map[string]string{}
	addresses := map[string]bool{}
	for section := 0; section < 3; section++ {
		count := int(binary.BigEndian.Uint16(response[6+2*section : 8+2*section]))
		for record := 0; record < count; record++ {
			owner, next, err := dnsReadinessName(response, offset)
			if err != nil || next+10 > len(response) {
				return errors.New("DNS readiness response record malformed")
			}
			typ := binary.BigEndian.Uint16(response[next : next+2])
			class := binary.BigEndian.Uint16(response[next+2 : next+4])
			size := int(binary.BigEndian.Uint16(response[next+8 : next+10]))
			start, end := next+10, next+10+size
			if end > len(response) {
				return errors.New("DNS readiness response record truncated")
			}
			switch typ {
			case 1: // A
				if size != 4 {
					return errors.New("DNS readiness response A record malformed")
				}
				if section == 0 && class == 1 {
					addresses[owner] = true
				}
			case 5: // CNAME
				alias, aliasEnd, err := dnsReadinessName(response, start)
				if err != nil || aliasEnd != end {
					return errors.New("DNS readiness response CNAME malformed")
				}
				if section == 0 && class == 1 {
					aliases[owner] = alias
				}
			case 41: // OPT: high byte of TTL carries the extended rcode.
				if response[next+4] != 0 {
					return errors.New("DNS readiness response extended rcode invalid")
				}
			}
			offset = end
		}
	}
	if offset != len(response) {
		return errors.New("DNS readiness response has trailing data")
	}
	// An unrelated answer, a bare CNAME, or an A record only in the additional
	// section does not establish resolution of our requested bootstrap host.
	for hops := 0; hops <= len(aliases); hops++ {
		if addresses[requested] {
			return nil
		}
		requested = aliases[requested]
		if requested == "" {
			break
		}
	}
	return errors.New("DNS readiness response has no resolved A answer")
}

// Decode RFC 1035 names without trusting compression offsets or lengths. The
// maximum packet/name sizes and strictly backwards pointers bound parser work.
func dnsReadinessName(packet []byte, offset int) (string, int, error) {
	var labels []string
	end, size := -1, 0
	for steps := 0; steps < len(packet); steps++ {
		if offset < 0 || offset >= len(packet) {
			break
		}
		length := int(packet[offset])
		if length&0xc0 == 0xc0 {
			if offset+1 >= len(packet) {
				break
			}
			pointer := (length&0x3f)<<8 | int(packet[offset+1])
			if pointer < 12 || pointer >= offset {
				break
			}
			if end == -1 {
				end = offset + 2
			}
			offset = pointer
			continue
		}
		if length&0xc0 != 0 {
			break
		}
		offset++
		if length == 0 {
			if end == -1 {
				end = offset
			}
			return strings.ToLower(strings.Join(labels, ".")), end, nil
		}
		if offset+length > len(packet) {
			break
		}
		size += length + 1
		if size > 254 {
			break
		}
		label := packet[offset : offset+length]
		for _, b := range label {
			if b < 0x21 || b > 0x7e || b == '.' {
				return "", 0, errors.New("DNS readiness name label invalid")
			}
		}
		labels = append(labels, string(label))
		offset += length
	}
	return "", 0, errors.New("DNS readiness name malformed")
}
