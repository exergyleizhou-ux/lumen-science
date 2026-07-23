package runtime

import (
	sciconfig "lumen/internal/science/config"
	"lumen/internal/science/launcher"
)

// Reload refreshes in-memory config from disk.
func (m *Manager) Reload() error {
	cfg, err := sciconfig.Load(m.SciDir)
	if err != nil {
		return err
	}
	m.mu.Lock()
	m.cfg = cfg
	m.mu.Unlock()
	return nil
}

// OneClick starts proxy+sandbox and returns sandbox URL and action label.
func (m *Manager) OneClick(openBrowser bool) (url, action, msg string, err error) {
	url, action, err = m.StartSandbox()
	if err != nil {
		return "", "", "", err
	}
	switch action {
	case "reopened":
		if m.lastProxyAction == ProxyRestarted {
			msg = "沙箱已在运行；代理已用新配置重启，Science 沿用不变"
		} else {
			msg = "沙箱已在运行，已重新打开"
		}
	case "started":
		msg = "Science 沙箱已启动"
	default:
		msg = "Science 已就绪"
	}
	if openBrowser {
		if err := launcher.OpenBrowser(url); err != nil {
			msg += "（浏览器未能自动打开，请手动访问）"
		}
	}
	return url, action, msg, nil
}
