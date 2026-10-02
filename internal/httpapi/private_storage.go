package httpapi

import (
	"context"
	"path/filepath"
)

func (s *Server) writePrivate(ctx context.Context, path string, raw []byte, recovery bool) error {
	if s.storageAdmission != nil {
		release, err := s.storageAdmission(ctx, filepath.Dir(path), int64(len(raw))+4096, recovery)
		if err != nil {
			return err
		}
		defer release()
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	return writePrivateFile(path, raw)
}
