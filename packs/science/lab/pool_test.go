package lab

import (
	"fmt"
	"sync"
	"testing"
	"time"

	"lumen/internal/science/lab/project"
)

func TestTurnPoolCapacity(t *testing.T) {
	p := newTurnPool(2)
	if !p.tryAcquire() || !p.tryAcquire() {
		t.Fatal("should acquire 2")
	}
	if p.tryAcquire() {
		t.Fatal("third should fail")
	}
	p.release()
	if !p.tryAcquire() {
		t.Fatal("after release should acquire")
	}
}

func TestControllerPoolBusySameProject(t *testing.T) {
	dir := t.TempDir()
	store := project.NewStore(dir)
	pool := newControllerPool(dir, nil, store, 4)
	c1, err := pool.acquire("proj-a")
	if err != nil || c1 == nil {
		t.Fatalf("first acquire: %v", err)
	}
	_, err = pool.acquire("proj-a")
	if err == nil {
		t.Fatal("same project concurrent should fail")
	}
	pool.release("proj-a")
	c2, err := pool.acquire("proj-a")
	if err != nil || c2 == nil {
		t.Fatalf("after release: %v", err)
	}
	pool.release("proj-a")
}

func TestControllerPoolParallelProjects(t *testing.T) {
	dir := t.TempDir()
	store := project.NewStore(dir)
	pool := newControllerPool(dir, nil, store, 8)
	var wg sync.WaitGroup
	errCh := make(chan error, 4)
	for i := 0; i < 4; i++ {
		wg.Add(1)
		slug := fmt.Sprintf("p%d", i)
		go func(s string) {
			defer wg.Done()
			c, err := pool.acquire(s)
			if err != nil {
				errCh <- err
				return
			}
			time.Sleep(20 * time.Millisecond)
			pool.release(s)
			if c == nil {
				errCh <- fmt.Errorf("nil controller")
			}
		}(slug)
	}
	wg.Wait()
	close(errCh)
	for e := range errCh {
		if e != nil {
			t.Fatal(e)
		}
	}
}
