import { Sun, Moon, Monitor, FolderOpen, Info } from "lucide-react";
import { cn } from "@/lib/utils";
import { useThemeStore } from "@/stores/themeStore";

export function SettingsPage() {
  const { theme, setTheme } = useThemeStore();

  const themeOptions = [
    { id: "light" as const, label: "浅色", icon: Sun },
    { id: "dark" as const, label: "深色", icon: Moon },
    { id: "system" as const, label: "跟随系统", icon: Monitor },
  ];

  return (
    <div className="p-8 max-w-3xl mx-auto">
      {/* Header */}
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-foreground">设置</h1>
        <p className="text-sm text-muted-foreground mt-1">应用偏好设置</p>
      </div>

      {/* Theme Selection */}
      <section className="mb-8">
        <h2 className="text-lg font-semibold text-foreground mb-4">外观</h2>
        <div className="bg-card border border-border rounded-xl p-5">
          <p className="text-sm text-muted-foreground mb-3">选择主题</p>
          <div className="flex gap-3">
            {themeOptions.map((option) => (
              <button
                key={option.id}
                onClick={() => setTheme(option.id)}
                className={cn(
                  "flex-1 flex flex-col items-center gap-2 p-4 rounded-xl border-2 transition-all",
                  theme === option.id
                    ? "border-primary bg-primary/5"
                    : "border-border hover:border-primary/30"
                )}
              >
                <option.icon
                  className={cn(
                    "w-6 h-6",
                    theme === option.id
                      ? "text-primary"
                      : "text-muted-foreground"
                  )}
                />
                <span
                  className={cn(
                    "text-sm font-medium",
                    theme === option.id
                      ? "text-primary"
                      : "text-muted-foreground"
                  )}
                >
                  {option.label}
                </span>
              </button>
            ))}
          </div>
        </div>
      </section>

      {/* Chrome Settings */}
      <section className="mb-8">
        <h2 className="text-lg font-semibold text-foreground mb-4">Chrome 浏览器</h2>
        <div className="bg-card border border-border rounded-xl p-5 space-y-4">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-sm font-medium text-foreground">
                Chrome 路径
              </p>
              <p className="text-xs text-muted-foreground">
                自动检测 Chrome 安装位置
              </p>
            </div>
            <button className="inline-flex items-center gap-1.5 px-3 py-1.5 bg-secondary rounded-lg text-xs text-foreground hover:bg-accent transition-colors">
              <FolderOpen className="w-3.5 h-3.5" />
              选择路径
            </button>
          </div>
          <div className="flex items-center justify-between">
            <div>
              <p className="text-sm font-medium text-foreground">
                Profile 存储目录
              </p>
              <p className="text-xs text-muted-foreground font-mono">
                ~/.multi-publisher/profiles/
              </p>
            </div>
            <button className="inline-flex items-center gap-1.5 px-3 py-1.5 bg-secondary rounded-lg text-xs text-foreground hover:bg-accent transition-colors">
              <FolderOpen className="w-3.5 h-3.5" />
              打开目录
            </button>
          </div>
        </div>
      </section>

      {/* About */}
      <section>
        <h2 className="text-lg font-semibold text-foreground mb-4">关于</h2>
        <div className="bg-card border border-border rounded-xl p-5">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-xl bg-primary flex items-center justify-center">
              <span className="text-primary-foreground font-bold text-sm">
                MP
              </span>
            </div>
            <div>
              <p className="text-sm font-semibold text-foreground">
                MultiPublisher
              </p>
              <p className="text-xs text-muted-foreground">v0.1.0</p>
            </div>
          </div>
          <div className="mt-4 p-3 bg-secondary/50 rounded-lg flex items-start gap-2">
            <Info className="w-4 h-4 text-muted-foreground mt-0.5 shrink-0" />
            <p className="text-xs text-muted-foreground leading-relaxed">
              MultiPublisher 是一个多平台视频发布工具。所有登录数据由 Chrome
              浏览器管理，本应用不存储任何密码或登录凭证。
            </p>
          </div>
        </div>
      </section>
    </div>
  );
}
