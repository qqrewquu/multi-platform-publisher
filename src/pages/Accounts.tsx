import { useState } from "react";
import { Plus, RefreshCw, ExternalLink, Pencil, Trash2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { useAccountStore } from "@/stores/accountStore";
import { PlatformIcon, PlatformBadge } from "@/components/PlatformIcon";
import { PLATFORMS } from "@/types";
import type { PlatformType } from "@/types";

const platformFilters: { id: PlatformType | "all"; label: string }[] = [
  { id: "all", label: "全部" },
  { id: "douyin", label: "抖音" },
  { id: "xiaohongshu", label: "小红书" },
  { id: "wechat", label: "微信视频号" },
  { id: "bilibili", label: "哔哩哔哩" },
  { id: "youtube", label: "YouTube" },
];

export function Accounts() {
  const { accounts, removeAccount } = useAccountStore();
  const [activeFilter, setActiveFilter] = useState<PlatformType | "all">("all");

  const filteredAccounts =
    activeFilter === "all"
      ? accounts
      : accounts.filter((a) => a.platform === activeFilter);

  return (
    <div className="p-8 max-w-5xl mx-auto">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-foreground">账号管理</h1>
          <p className="text-sm text-muted-foreground mt-1">
            管理你的各平台创作者账号
          </p>
        </div>
        <div className="flex gap-2">
          <button className="inline-flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-medium hover:opacity-90 transition-opacity">
            <Plus className="w-4 h-4" />
            添加账号
          </button>
          <button className="inline-flex items-center gap-2 px-4 py-2 bg-secondary text-secondary-foreground rounded-lg text-sm font-medium hover:bg-accent transition-colors border border-border">
            <RefreshCw className="w-4 h-4" />
            状态检查
          </button>
        </div>
      </div>

      {/* Platform Filter Tabs */}
      <div className="flex gap-1 mb-6 bg-secondary/50 p-1 rounded-lg w-fit">
        {platformFilters.map((filter) => {
          const count =
            filter.id === "all"
              ? accounts.length
              : accounts.filter((a) => a.platform === filter.id).length;
          return (
            <button
              key={filter.id}
              onClick={() => setActiveFilter(filter.id)}
              className={cn(
                "px-3 py-1.5 rounded-md text-sm transition-colors",
                activeFilter === filter.id
                  ? "bg-card text-foreground font-medium shadow-sm"
                  : "text-muted-foreground hover:text-foreground"
              )}
            >
              {filter.label}
              {count > 0 && (
                <span className="ml-1 text-xs text-muted-foreground">
                  ({count})
                </span>
              )}
            </button>
          );
        })}
      </div>

      {/* Account Cards Grid */}
      {filteredAccounts.length === 0 ? (
        <div className="bg-card border border-border rounded-xl p-12 text-center">
          <p className="text-muted-foreground">该平台暂无账号</p>
          <button className="mt-3 text-sm text-primary hover:text-primary/80 transition-colors">
            添加账号
          </button>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {filteredAccounts.map((account) => {
            const info = PLATFORMS[account.platform];
            return (
              <div
                key={account.id}
                className="bg-card border border-border rounded-xl p-5 hover:border-primary/30 transition-colors group"
              >
                {/* Top Row: Icon + Info */}
                <div className="flex items-start gap-3 mb-4">
                  <PlatformIcon platform={account.platform} size="lg" />
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <PlatformBadge platform={account.platform} />
                    </div>
                    <p className="text-sm font-medium text-foreground mt-1 truncate">
                      {account.displayName}
                    </p>
                    <div className="flex items-center gap-1.5 mt-1">
                      <div
                        className={cn(
                          "w-2 h-2 rounded-full",
                          account.isLoggedIn ? "bg-green-500" : "bg-amber-500"
                        )}
                      />
                      <span
                        className={cn(
                          "text-xs",
                          account.isLoggedIn
                            ? "text-green-600 dark:text-green-400"
                            : "text-amber-600 dark:text-amber-400"
                        )}
                      >
                        {account.isLoggedIn ? "已登录" : "需要重新登录"}
                      </span>
                    </div>
                  </div>
                </div>

                {/* Actions */}
                <div className="flex gap-2 pt-3 border-t border-border">
                  <ActionButton
                    icon={ExternalLink}
                    label="打开"
                    onClick={() =>
                      window.open(info.creatorUrl, "_blank")
                    }
                  />
                  <ActionButton icon={Pencil} label="编辑" />
                  <ActionButton icon={RefreshCw} label="检查状态" />
                  <ActionButton
                    icon={Trash2}
                    label="删除"
                    variant="destructive"
                    onClick={() => removeAccount(account.id)}
                  />
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function ActionButton({
  icon: Icon,
  label,
  variant = "default",
  onClick,
}: {
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  variant?: "default" | "destructive";
  onClick?: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "flex-1 inline-flex items-center justify-center gap-1 px-2 py-1.5 rounded-md text-xs transition-colors",
        variant === "destructive"
          ? "text-destructive hover:bg-destructive/10"
          : "text-muted-foreground hover:bg-secondary hover:text-foreground"
      )}
    >
      <Icon className="w-3.5 h-3.5" />
      {label}
    </button>
  );
}
