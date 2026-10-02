package traffic

import (
	"context"
	"encoding/binary"
	"errors"
	"fmt"
	"hash/crc32"
	"io"
	"math"
	"os"
	"path/filepath"
)

const headerSize = 64
const recordSize = 64
const recordMagic = uint32(0x54465242)

// Each logical bucket has two physical slots. A torn overwrite leaves the
// previous checksum-valid generation available. No archive is rewritten.
type bucket struct {
	start            int64
	rx, tx, coverage uint64 // coverage is nanoseconds, not a count of samples
	rxPeak, txPeak   float64
	generation       uint64
	endMillis        uint32 // observed end offset within the bucket, rounded up
	valid            bool
}
type ring struct {
	seconds  int64
	capacity int
	file     *os.File
	buckets  []bucket
	dirty    []bool
}

var layouts = []struct {
	seconds  int64
	capacity int
}{
	{30, 5760}, {300, 8640}, {3600, 9600},
}

func openRing(dir string, seconds int64, capacity int, ctx context.Context, writeAt func(*os.File, []byte, int64) (int, error)) (*ring, bool, error) {
	path := filepath.Join(dir, fmt.Sprintf("wan-%ds.ring", seconds))
	f, err := os.OpenFile(path, os.O_RDWR, 0600)
	if errors.Is(err, os.ErrNotExist) {
		if err = createRing(dir, path, seconds, capacity, ctx, writeAt); err != nil {
			return nil, false, err
		}
		f, err = os.OpenFile(path, os.O_RDWR, 0600)
	}
	if err != nil {
		return nil, false, err
	}
	success := false
	defer func() {
		if !success {
			_ = f.Close()
		}
	}()
	info, err := f.Stat()
	size := int64(headerSize + capacity*recordSize*2)
	if err != nil {
		return nil, false, err
	}
	if !info.Mode().IsRegular() || info.Size() > size || info.Size() < headerSize {
		return nil, false, errors.New("traffic ring has invalid size or file type")
	}
	var header [headerSize]byte
	if _, err = f.ReadAt(header[:], 0); err != nil {
		return nil, false, err
	}
	if string(header[:8]) != "WANRING\x00" || int64(binary.LittleEndian.Uint64(header[8:16])) != seconds || int(binary.LittleEndian.Uint64(header[16:24])) != capacity || binary.LittleEndian.Uint32(header[60:64]) != crc32.ChecksumIEEE(header[:60]) {
		return nil, false, errors.New("traffic ring header is corrupt or incompatible")
	}
	data := make([]byte, capacity*recordSize*2)
	n, err := f.ReadAt(data, headerSize)
	if err != nil && err != io.EOF {
		return nil, false, err
	}
	recovered := n != len(data)
	r := &ring{seconds: seconds, capacity: capacity, file: f, buckets: make([]bucket, capacity), dirty: make([]bool, capacity)}
	for i := 0; i < capacity; i++ {
		for copyIndex := 0; copyIndex < 2; copyIndex++ {
			offset := (i*2 + copyIndex) * recordSize
			raw := data[offset : offset+recordSize]
			b, ok := decodeBucket(raw, seconds, capacity, i)
			if !ok {
				for _, v := range raw {
					if v != 0 {
						recovered = true
						break
					}
				}
				continue
			}
			if !r.buckets[i].valid || b.generation > r.buckets[i].generation {
				r.buckets[i] = b
			}
		}
	}
	// Restore a truncated tail with real writes. ENOSPC must be visible, not
	// hidden by sparse allocation. Valid earlier records have already survived.
	if info.Size() < size {
		if err = writeZeros(ctx, f, info.Size(), size-info.Size(), writeAt); err != nil {
			return nil, false, err
		}
		if err = f.Sync(); err != nil {
			return nil, false, err
		}
	}
	success = true
	return r, recovered, nil
}

func createRing(dir, path string, seconds int64, capacity int, ctx context.Context, writeAt func(*os.File, []byte, int64) (int, error)) error {
	f, err := os.CreateTemp(dir, ".wan-ring-")
	if err != nil {
		return err
	}
	temporary := f.Name()
	defer os.Remove(temporary)
	var header [headerSize]byte
	copy(header[:8], "WANRING\x00")
	binary.LittleEndian.PutUint64(header[8:16], uint64(seconds))
	binary.LittleEndian.PutUint64(header[16:24], uint64(capacity))
	binary.LittleEndian.PutUint32(header[60:64], crc32.ChecksumIEEE(header[:60]))
	if _, err = f.Write(header[:]); err == nil {
		err = writeZeros(ctx, f, headerSize, int64(capacity*recordSize*2), writeAt)
	}
	if err == nil {
		err = f.Sync()
	}
	closeErr := f.Close()
	if err != nil {
		return err
	}
	if closeErr != nil {
		return closeErr
	}
	if err = ctx.Err(); err != nil {
		return err
	}
	if err = os.Rename(temporary, path); err != nil {
		return err
	}
	directory, err := os.Open(dir)
	if err != nil {
		return err
	}
	defer directory.Close()
	return directory.Sync()
}

func writeZeros(ctx context.Context, f *os.File, offset, length int64, writeAt func(*os.File, []byte, int64) (int, error)) error {
	if writeAt == nil {
		writeAt = func(f *os.File, p []byte, offset int64) (int, error) { return f.WriteAt(p, offset) }
	}
	zeros := make([]byte, 32<<10)
	for length > 0 {
		if err := ctx.Err(); err != nil {
			return err
		}
		n := int64(len(zeros))
		if length < n {
			n = length
		}
		wrote, err := writeAt(f, zeros[:int(n)], offset)
		if err != nil {
			return err
		}
		if wrote != int(n) {
			return io.ErrShortWrite
		}
		offset += n
		length -= n
	}
	return nil
}

func decodeBucket(raw []byte, seconds int64, capacity, index int) (bucket, bool) {
	if binary.LittleEndian.Uint32(raw[60:64]) != recordMagic || binary.LittleEndian.Uint32(raw[56:60]) != crc32.ChecksumIEEE(raw[:56]) {
		return bucket{}, false
	}
	b := bucket{generation: uint64(binary.LittleEndian.Uint32(raw[:4])), endMillis: binary.LittleEndian.Uint32(raw[4:8]), start: int64(binary.LittleEndian.Uint64(raw[8:16])), rx: binary.LittleEndian.Uint64(raw[16:24]), tx: binary.LittleEndian.Uint64(raw[24:32]), coverage: binary.LittleEndian.Uint64(raw[32:40]), rxPeak: math.Float64frombits(binary.LittleEndian.Uint64(raw[40:48])), txPeak: math.Float64frombits(binary.LittleEndian.Uint64(raw[48:56])), valid: true}
	if b.generation == 0 || b.endMillis == 0 || b.endMillis > uint32(seconds*1000) || b.start < 0 || b.start%seconds != 0 || int((b.start/seconds)%int64(capacity)) != index || b.coverage > uint64(seconds)*1e9 || math.IsNaN(b.rxPeak) || math.IsInf(b.rxPeak, 0) || b.rxPeak < 0 || math.IsNaN(b.txPeak) || math.IsInf(b.txPeak, 0) || b.txPeak < 0 || (b.coverage == 0 && (b.rx != 0 || b.tx != 0)) {
		return bucket{}, false
	}
	return b, true
}

func encodeBucket(b bucket) [recordSize]byte {
	var raw [recordSize]byte
	binary.LittleEndian.PutUint32(raw[:4], uint32(b.generation))
	binary.LittleEndian.PutUint32(raw[4:8], b.endMillis)
	binary.LittleEndian.PutUint64(raw[8:16], uint64(b.start))
	binary.LittleEndian.PutUint64(raw[16:24], b.rx)
	binary.LittleEndian.PutUint64(raw[24:32], b.tx)
	binary.LittleEndian.PutUint64(raw[32:40], b.coverage)
	binary.LittleEndian.PutUint64(raw[40:48], math.Float64bits(b.rxPeak))
	binary.LittleEndian.PutUint64(raw[48:56], math.Float64bits(b.txPeak))
	binary.LittleEndian.PutUint32(raw[56:60], crc32.ChecksumIEEE(raw[:56]))
	binary.LittleEndian.PutUint32(raw[60:64], recordMagic)
	return raw
}

func (r *ring) flush() error {
	changed := false
	for i, dirty := range r.dirty {
		if !dirty {
			continue
		}
		b := r.buckets[i]
		b.generation++
		raw := encodeBucket(b)
		offset := int64(headerSize + (i*2+int(b.generation%2))*recordSize)
		if n, err := r.file.WriteAt(raw[:], offset); err != nil {
			return err
		} else if n != recordSize {
			return io.ErrShortWrite
		}
		// Do not advance committed generations before Sync succeeds.
		changed = true
	}
	if !changed {
		return nil
	}
	if err := r.file.Sync(); err != nil {
		return err
	}
	for i, dirty := range r.dirty {
		if dirty {
			r.buckets[i].generation++
			r.dirty[i] = false
		}
	}
	return nil
}
