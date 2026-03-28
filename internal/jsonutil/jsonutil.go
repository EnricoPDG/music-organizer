package jsonutil

// Get walks a chain of string keys through nested map[string]any values.
// Returns nil if any step in the chain is missing or not a map.
func Get(v any, keys ...string) any {
	for _, k := range keys {
		m, ok := v.(map[string]any)
		if !ok {
			return nil
		}
		v = m[k]
	}
	return v
}

// Idx safely indexes into a []any value.
// Returns nil if v is not a slice or i is out of bounds.
func Idx(v any, i int) any {
	a, ok := v.([]any)
	if !ok || i >= len(a) {
		return nil
	}
	return a[i]
}
