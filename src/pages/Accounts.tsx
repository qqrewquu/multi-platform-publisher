import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Plus,
  RefreshCw,
  ExternalLink,
  Pencil,
  Trash2,
  LogIn,
  Chrome,
  CheckCircle,
  AlertCircle,
  X,
} from "lucide-react";
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
  const { accounts, fetchAccounts, addAccount, removeAccount, openLogin, openPlatform, updateLoginStatus } =
    useAccountStore();
  const [activeFilter, setActiveFilter] = useState<PlatformType | "all">("all");
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [chromeFound, setChromeFound] = useState<boolean | null>(null);
  const [loginPendingAccountId, setLoginPendingAccountId] = useState<number | null>(null);

  useEffect(() => {
    fetchAccounts();
    // Check Chrome installation
    invoke<{ found: boolean }>("detect_chrome").then((res) => {
      setChromeFound(res.found);
    });
  }, []);

  const filteredAccounts =
    activeFilter === "all"
      ? accounts
      : accounts.filter((a) => a.platform === activeFilter);

  return (
    <div className="p-8 max-w-5xl mx-auto">
      {/* Chrome Status Banner */}
      {chromeFound === false && (
        <div className="mb-4 p-3 bg-destructive/10 border border-destructive/20 rounded-lg flex items-center gap-2">
          <AlertCircle className="w-4 h-4 text-destructive shrink-0" />
          <p className="text-sm text-destructive">
            未检测到 Chrome 浏览器，请先安装 Google Chrome
          </p>
        </div>
      )}
      {chromeFound === true && (
        <div className="mb-4 p-3 bg-green-500/10 border border-green-500/20 rounded-lg flex items-center gap-2">
          <Chrome className="w-4 h-4 text-green-600 dark:text-green-400 shrink-0" />
          <p className="text-sm text-green-600 dark:text-green-400">
            Chrome 浏览器已就绪
          </p>
        </div>
      )}

      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-foreground">账号管理</h1>
          <p className="text-sm text-muted-foreground mt-1">
            管理你的各平台创作者账号
          </p>
        </div>
        <div className="flex gap-2">
          <button
            onClick={() => setShowAddDialog(true)}
            className="inline-flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-medium hover:opacity-90 transition-opacity"
          >
            <Plus className="w-4 h-4" />
            添加账号
          </button>
          <button
            onClick={fetchAccounts}
            className="inline-flex items-center gap-2 px-4 py-2 bg-secondary text-secondary-foreground rounded-lg text-sm font-medium hover:bg-accent transition-colors border border-border"
          >
            <RefreshCw className="w-4 h-4" />
            刷新
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
          <p className="text-muted-foreground">
            {accounts.length === 0
              ? "还没有添加账号，点击上方「添加账号」开始"
              : "该平台暂无账号"}
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {filteredAccounts.map((account) => {
            const info = PLATFORMS[account.platform];
            return (
              <div
                key={account.id}
                className="bg-card border border-border rounded-xl p-5 hover:border-primary/30 transition-colors"
              >
                <div className="flex items-start gap-3 mb-4">
                  <PlatformIcon platform={account.platform} size="lg" />
                  <div className="flex-1 min-w-0">
                    <PlatformBadge platform={account.platform} />
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
                        {account.isLoggedIn ? "已登录" : "需要登录"}
                      </span>
                    </div>
                  </div>
                </div>

                <div className="flex gap-2 pt-3 border-t border-border">
                  {account.isLoggedIn ? (
                    <ActionButton
                      icon={CheckCircle}
                      label="已登录"
                      variant="success"
                    />
                  ) : (
                    <ActionButton
                      icon={LogIn}
                      label="登录"
                      onClick={async () => {
                        await openLogin(account.id);
                        setLoginPendingAccountId(account.id);
                      }}
                    />
                  )}
                  <ActionButton
                    icon={ExternalLink}
                    label="打开"
                    onClick={() => openPlatform(account.id)}
                  />
                  <ActionButton
                    icon={Trash2}
                    label="删除"
                    variant="destructive"
                    onClick={() => {
                      if (confirm("确定要删除此账号吗？Chrome 登录数据也会被清除。")) {
                        removeAccount(account.id);
                      }
                    }}
                  />
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* Add Account Dialog */}
      {showAddDialog && (
        <AddAccountDialog
          onClose={() => setShowAddDialog(false)}
          onAdd={async (platform) => {
            await addAccount(platform, "");
            setShowAddDialog(false);
          }}
        />
      )}

      {/* Login Confirmation Dialog */}
      {loginPendingAccountId !== null && (
        <LoginConfirmDialog
          account={accounts.find((a) => a.id === loginPendingAccountId)}
          onConfirm={async () => {
            await updateLoginStatus(loginPendingAccountId, true);
            setLoginPendingAccountId(null);
          }}
          onCancel={() => setLoginPendingAccountId(null)}
        />
      )}
    </div>
  );
}

function AddAccountDialog({
  onClose,
  onAdd,
}: {
  onClose: () => void;
  onAdd: (platform: PlatformType) => Promise<void>;
}) {
  const platforms: PlatformType[] = [
    "douyin",
    "xiaohongshu",
    "bilibili",
    "wechat",
    "youtube",
  ];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-card border border-border rounded-2xl p-6 w-[400px] shadow-2xl">
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-lg font-semibold text-foreground">添加账号</h2>
          <button
            onClick={onClose}
            className="p-1 hover:bg-secondary rounded-lg transition-colors"
          >
            <X className="w-5 h-5 text-muted-foreground" />
          </button>
        </div>
        <p className="text-sm text-muted-foreground mb-4">
          选择平台，将打开 Chrome 浏览器让你登录
        </p>
        <div className="space-y-2">
          {platforms.map((platform) => {
            const info = PLATFORMS[platform];
            return (
              <button
                key={platform}
                onClick={() => onAdd(platform)}
                className="w-full flex items-center gap-3 p-3 rounded-xl hover:bg-secondary transition-colors text-left"
              >
                <PlatformIcon platform={platform} size="md" />
                <div>
                  <p className="text-sm font-medium text-foreground">
                    {info.name}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    {info.creatorUrl}
                  </p>
                </div>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function LoginConfirmDialog({
  account,
  onConfirm,
  onCancel,
}: {
  account: ReturnType<typeof useAccountStore.getState>["accounts"][0] | undefined;
  onConfirm: () => Promise<void>;
  onCancel: () => void;
}) {
  if (!account) return null;
  const info = PLATFORMS[account.platform];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-card border border-border rounded-2xl p-6 w-[420px] shadow-2xl">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-lg font-semibold text-foreground">确认登录状态</h2>
          <button
            onClick={onCancel}
            className="p-1 hover:bg-secondary rounded-lg transition-colors"
          >
            <X className="w-5 h-5 text-muted-foreground" />
          </button>
        </div>

        <div className="flex items-center gap-3 p-3 bg-secondary/50 rounded-lg mb-4">
          <Chrome className="w-8 h-8 text-primary shrink-0" />
          <div>
            <p className="text-sm font-medium text-foreground">
              Chrome 已打开 {info.name}
            </p>
            <p className="text-xs text-muted-foreground">
              请在 Chrome 浏览器中完成扫码或密码登录
            </p>
          </div>
        </div>

        <p className="text-sm text-muted-foreground mb-5">
          登录完成后，点击下方按钮确认。如果还没登录完成，可以先点「稍后确认」，之后在账号卡片上手动标记。
        </p>

        <div className="flex gap-3">
          <button
            onClick={onCancel}
            className="flex-1 py-2.5 rounded-xl text-sm font-medium border border-border text-foreground hover:bg-secondary transition-colors"
          >
            稍后确认
          </button>
          <button
            onClick={onConfirm}
            className="flex-1 py-2.5 rounded-xl text-sm font-medium bg-green-600 text-white hover:bg-green-700 transition-colors"
          >
            已完成登录
          </button>
        </div>
      </div>
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
  variant?: "default" | "destructive" | "success";
  onClick?: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "flex-1 inline-flex items-center justify-center gap-1 px-2 py-1.5 rounded-md text-xs transition-colors",
        variant === "destructive"
          ? "text-destructive hover:bg-destructive/10"
          : variant === "success"
          ? "text-green-600 dark:text-green-400"
          : "text-muted-foreground hover:bg-secondary hover:text-foreground"
      )}
    >
      <Icon className="w-3.5 h-3.5" />
      {label}
    </button>
  );
}
