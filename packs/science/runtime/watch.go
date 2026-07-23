package runtime

import (
	"fmt"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"
)

// WatchCache polls proxy /health and prints live DeepSeek prefix-cache hit rates.
func (m *Manager) WatchCache(interval time.Duration) error {
	if interval <= 0 {
		interval = 2 * time.Second
	}
	if _, err := m.StartProxy(); err != nil {
		return err
	}
	sig := make(chan os.Signal, 1)
	signal.Notify(sig, syscall.SIGINT, syscall.SIGTERM)
	tick := time.NewTicker(interval)
	defer tick.Stop()

	fmt.Println("DeepSeek 前缀缓存监控（Ctrl+C 退出，沙箱保持运行）")
	fmt.Println(strings.Repeat("─", 52))
	for {
		select {
		case <-sig:
			fmt.Println("\n监控已停止。")
			return nil
		case <-tick.C:
			st := m.Status()
			proxyOK, _ := st["proxy_healthy"].(bool)
			if !proxyOK {
				fmt.Printf("\r[%s] 代理未就绪…", time.Now().Format("15:04:05"))
				continue
			}
			session, _ := st["cache_session_hit_pct"].(int64)
			last, _ := st["cache_last_hit_pct"].(int64)
			hits, _ := st["cache_hit_tokens"].(int64)
			bar := cacheBar(int(session))
			fmt.Printf("\r[%s] 会话命中 %3d%% %s | 上次 %3d%% | 命中 token %d   ",
				time.Now().Format("15:04:05"), session, bar, last, hits)
		}
	}
}

func cacheBar(pct int) string {
	if pct < 0 {
		pct = 0
	}
	if pct > 100 {
		pct = 100
	}
	filled := pct / 10
	if filled > 10 {
		filled = 10
	}
	return strings.Repeat("█", filled) + strings.Repeat("░", 10-filled)
}
