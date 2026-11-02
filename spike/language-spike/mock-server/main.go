// Command mock is the shared HTTP mock used by all three spike implementations.
//
// Routes (any first path segment is a "resource"):
//
//	POST   /{res}            -> 201 entity (id unique per server)
//	GET    /{res}            -> 200 {items:[...], count}
//	GET    /{res}/{id}       -> 200 entity with that id ("missing" -> 404)
//	PUT    /{res}/{id}       -> 200 entity with new name
//	DELETE /{res}/{id}       -> 200 {deleted:true}
//	POST   /slow/{res}       -> like POST but sleeps 50ms first (exercises singleflight)
//	GET    /__stats          -> per-endpoint request counts  (not counted)
//	POST   /__reset          -> zero the counters            (not counted)
//
// Counters key on "METHOD /first[/second]/:id" so ids collapse.
package main

import (
	"encoding/json"
	"flag"
	"io"
	"log"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

type counters struct {
	m sync.Map // string -> *atomic.Int64
}

func (c *counters) inc(key string) {
	v, ok := c.m.Load(key)
	if !ok {
		v, _ = c.m.LoadOrStore(key, new(atomic.Int64))
	}
	v.(*atomic.Int64).Add(1)
}

func (c *counters) snapshot() map[string]int64 {
	out := map[string]int64{}
	c.m.Range(func(k, v any) bool {
		out[k.(string)] = v.(*atomic.Int64).Load()
		return true
	})
	return out
}

func (c *counters) reset() { c.m.Clear() }

type item struct {
	Sku   string  `json:"sku"`
	Price float64 `json:"price"`
}

type entity struct {
	ID     string  `json:"id"`
	Name   string  `json:"name"`
	Items  []item  `json:"items"`
	Total  float64 `json:"total"`
	Active bool    `json:"active"`
}

var defaultItems = []item{{Sku: "a", Price: 9.5}, {Sku: "b", Price: 12.25}}

func newEntity(id, name string) entity {
	return entity{ID: id, Name: name, Items: defaultItems, Total: 21.75, Active: true}
}

type server struct {
	stats  counters
	nextID atomic.Int64
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func (s *server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	path := strings.Trim(r.URL.Path, "/")
	switch path {
	case "__stats":
		writeJSON(w, 200, s.stats.snapshot())
		return
	case "__reset":
		s.stats.reset()
		writeJSON(w, 200, map[string]bool{"ok": true})
		return
	}
	segs := strings.Split(path, "/")
	slow := len(segs) > 1 && segs[0] == "slow"
	key := r.Method + " /" + segs[0]
	if slow {
		key += "/" + segs[1]
	} else if len(segs) > 1 {
		key += "/:id"
	}
	s.stats.inc(key)

	var body struct {
		Name string `json:"name"`
	}
	if r.Body != nil {
		data, _ := io.ReadAll(r.Body)
		if len(data) > 0 {
			_ = json.Unmarshal(data, &body)
		}
	}
	if slow {
		time.Sleep(50 * time.Millisecond)
		segs = segs[:1] // treat as a create
	}
	switch {
	case r.Method == http.MethodPost && len(segs) == 1:
		id := segs[0] + "-" + strconv.FormatInt(s.nextID.Add(1), 10)
		writeJSON(w, 201, newEntity(id, body.Name))
	case r.Method == http.MethodGet && len(segs) == 1:
		writeJSON(w, 200, map[string]any{"items": []entity{newEntity("x-1", "one"), newEntity("x-2", "two")}, "count": 2})
	case r.Method == http.MethodGet && len(segs) == 2:
		if segs[1] == "missing" {
			writeJSON(w, 404, map[string]string{"error": "not found"})
			return
		}
		writeJSON(w, 200, newEntity(segs[1], "item-"+segs[1]))
	case r.Method == http.MethodPut && len(segs) == 2:
		writeJSON(w, 200, newEntity(segs[1], body.Name))
	case r.Method == http.MethodDelete && len(segs) == 2:
		writeJSON(w, 200, map[string]any{"id": segs[1], "deleted": true})
	default:
		writeJSON(w, 405, map[string]string{"error": "unsupported"})
	}
}

func main() {
	addr := flag.String("addr", "127.0.0.1:18080", "listen address")
	flag.Parse()
	srv := &http.Server{Addr: *addr, Handler: &server{}, ReadHeaderTimeout: 10 * time.Second}
	log.Printf("mock listening on %s", *addr)
	log.Fatal(srv.ListenAndServe())
}
