import { useState, useEffect, type KeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  Upload,
  X,
  Film,
  ChevronDown,
  ChevronUp,
  CheckCircle,
  AlertCircle,
  Loader2,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { usePublishStore } from "@/stores/publishStore";
import { useAccountStore } from "@/stores/accountStore";
import { PlatformIcon, PlatformBadge } from "@/components/PlatformIcon";
import { PLATFORMS } from "@/types";

interface PlatformTaskResult {
  account_id: number;
  platform: string;
  status: string;
  message?: string | null;
  error_code?: string | null;
  action_hint?: string | null;
  debug_port_used?: number | null;
  session_mode?: string | null;
  automation_phase?: "upload_started" | "manual_continue" | "automation_failed" | "timeout" | null;
}

interface PublishResponse {
  task_id: number;
  platform_tasks: PlatformTaskResult[];
}

export function NewPublish() {
  const {
    videoPath,
    videoName,
    title,
    description,
    tags,
    isOriginal,
    isScheduled,
    manualConfirm,
    selectedAccountIds,
    setVideoPath,
    setTitle,
    setDescription,
    addTag,
    removeTag,
    setIsOriginal,
    setIsScheduled,
    setManualConfirm,
    toggleAccount,
    selectAllAccounts,
    deselectAllAccounts,
    resetForm,
  } = usePublishStore();

  const { accounts, fetchAccounts } = useAccountStore();
  const [tagInput, setTagInput] = useState("");
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [isPublishing, setIsPublishing] = useState(false);
  const [publishResult, setPublishResult] = useState<PublishResponse | { error: string } | null>(null);

  useEffect(() => {
    fetchAccounts();

    // Listen for native Tauri file drag-and-drop
    const webview = getCurrentWebviewWindow();
    const unlisten = webview.onDragDropEvent((event) => {
      if (event.payload.type === "over") {
        setIsDragging(true);
      } else if (event.payload.type === "drop") {
        setIsDragging(false);
        const paths = event.payload.paths;
        if (paths && paths.length > 0) {
          const filePath = paths[0];
          const ext = filePath.split(".").pop()?.toLowerCase() || "";
          if (["mp4", "mov", "avi", "mkv", "webm", "flv"].includes(ext)) {
            const fileName = filePath.split("/").pop() || filePath.split("\\").pop() || filePath;
            setVideoPath(filePath, fileName);
          }
        }
      } else if (event.payload.type === "leave") {
        setIsDragging(false);
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const handleClickUpload = async () => {
    const selected = await openDialog({
      multiple: false,
      filters: [
        {
          name: "视频文件",
          extensions: ["mp4", "mov", "avi", "mkv", "webm", "flv"],
        },
      ],
    });
    if (selected) {
      const filePath = selected as string;
      const fileName = filePath.split("/").pop() || filePath.split("\\").pop() || filePath;
      setVideoPath(filePath, fileName);
    }
  };

  const handleTagKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && tagInput.trim()) {
      e.preventDefault();
      addTag(tagInput.trim());
      setTagInput("");
    }
  };

  const handlePublish = async () => {
    if (!videoPath || selectedAccountIds.length === 0 || !title.trim()) return;

    setIsPublishing(true);
    setPublishResult(null);
    try {
      const result = await invoke<PublishResponse>("create_publish_task", {
        request: {
          video_path: videoPath,
          title: title.trim(),
          description: description || null,
          tags,
          is_original: isOriginal,
          manual_confirm: manualConfirm,
          account_ids: selectedAccountIds,
        },
      });
      setPublishResult(result);
    } catch (e) {
      setPublishResult({ error: e instanceof Error ? e.message : String(e) });
    } finally {
      setIsPublishing(false);
    }
  };

  const selectedCount = selectedAccountIds.length;
  const canPublish = !!videoPath && selectedCount > 0 && !!title.trim() && !isPublishing;
  const hasResultError = !!publishResult && "error" in publishResult;
  const platformTasks = publishResult && "platform_tasks" in publishResult ? publishResult.platform_tasks : [];
  const hasTaskFailure = platformTasks.some((task) => task.status === "failed");
  const hasTaskWarning = platformTasks.some((task) => task.status === "launched");

  return (
    <div className="p-8 max-w-6xl mx-auto">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold text-foreground">新建发布任务</h1>
        <button
          onClick={() => {
            resetForm();
            setPublishResult(null);
          }}
          className="text-sm text-primary hover:text-primary/80 transition-colors"
        >
          重置表单
        </button>
      </div>

      {/* Publish Result Banner */}
      {publishResult && (
        <div
          className={cn(
            "mb-4 p-4 rounded-lg border flex items-start gap-3",
            hasResultError
              ? "bg-destructive/10 border-destructive/20"
              : hasTaskFailure || hasTaskWarning
                ? "bg-amber-500/10 border-amber-500/30"
                : "bg-green-500/10 border-green-500/20"
          )}
        >
          {hasResultError ? (
            <AlertCircle className="w-5 h-5 text-destructive shrink-0 mt-0.5" />
          ) : hasTaskFailure || hasTaskWarning ? (
            <AlertCircle className="w-5 h-5 text-amber-600 shrink-0 mt-0.5" />
          ) : (
            <CheckCircle className="w-5 h-5 text-green-600 dark:text-green-400 shrink-0 mt-0.5" />
          )}
          <div>
            {hasResultError ? (
              <p className="text-sm text-destructive">{(publishResult as { error: string }).error}</p>
            ) : (
              <>
                <p
                  className={cn(
                    "text-sm font-medium",
                    hasTaskFailure || hasTaskWarning
                      ? "text-amber-700 dark:text-amber-300"
                      : "text-green-600 dark:text-green-400"
                  )}
                >
                  {hasTaskFailure || hasTaskWarning ? "部分平台自动化未完成" : "Chrome 已启动"}
                </p>
                <p className="text-xs text-muted-foreground mt-1">
                  请在 Chrome 浏览器中检查每个平台并完成最终发布
                </p>
                {platformTasks.length > 0 && (
                  <div className="mt-2 space-y-1">
                    {platformTasks.map((task) => {
                      const platformInfo = PLATFORMS[task.platform as keyof typeof PLATFORMS];
                      const platformName = platformInfo?.name ?? task.platform;
                      const actionHint = resolveActionHint(task);
                      const phaseText = automationPhaseText(task);
                      const sessionMeta = formatSessionMeta(task);
                      const detail = task.message || platformStatusLabel(task.status);
                      const detailWithCode = task.error_code
                        ? `[${task.error_code}] ${detail}`
                        : detail;
                      return (
                        <p key={`${task.platform}-${task.account_id}`} className="text-xs text-muted-foreground">
                          {platformName}（账号 {task.account_id}）: {detailWithCode}
                          {phaseText ? ` 阶段：${phaseText}` : ""}
                          {sessionMeta ? ` 会话：${sessionMeta}` : ""}
                          {actionHint ? ` 建议：${actionHint}` : ""}
                        </p>
                      );
                    })}
                  </div>
                )}
              </>
            )}
          </div>
          <button
            onClick={() => setPublishResult(null)}
            className="ml-auto p-1 hover:bg-secondary rounded-md"
          >
            <X className="w-4 h-4 text-muted-foreground" />
          </button>
        </div>
      )}

      <div className="flex gap-6">
        {/* Left Column - Form */}
        <div className="flex-1 space-y-5">
          {/* Video Upload Area */}
          <div className="bg-card border border-border rounded-xl p-6">
            <label className="text-sm font-medium text-foreground mb-3 block">
              视频文件
            </label>
            {videoName ? (
              <div className="flex items-center gap-3 p-4 bg-secondary rounded-lg">
                <Film className="w-8 h-8 text-primary shrink-0" />
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium text-foreground truncate">
                    {videoName}
                  </p>
                  <p className="text-xs text-muted-foreground">仅已选择本地文件，未上传到本应用</p>
                </div>
                <button
                  onClick={() => setVideoPath(null, null)}
                  className="p-1 hover:bg-accent rounded-md transition-colors"
                >
                  <X className="w-4 h-4 text-muted-foreground" />
                </button>
              </div>
            ) : (
              <div
                onClick={handleClickUpload}
                className={cn(
                  "border-2 border-dashed rounded-xl p-10 text-center transition-all cursor-pointer",
                  isDragging
                    ? "border-primary bg-primary/5"
                    : "border-border hover:border-primary/50 hover:bg-secondary/50"
                )}
              >
                <Upload
                  className={cn(
                    "w-10 h-10 mx-auto mb-3",
                    isDragging ? "text-primary" : "text-muted-foreground"
                  )}
                />
                <p className="text-sm text-foreground">
                  拖拽视频文件到此处或{" "}
                  <span className="text-primary font-medium">点击上传</span>
                </p>
                <p className="text-xs text-muted-foreground mt-1">
                  支持 MP4, MOV, AVI 格式，最大 5GB
                </p>
                <p className="text-xs text-muted-foreground mt-1">
                  选择后会在 Chrome 平台页直接上传，不会先上传到本应用服务器
                </p>
              </div>
            )}
          </div>

          {/* Title & Description & Tags */}
          <div className="bg-card border border-border rounded-xl p-6 space-y-4">
            <div>
              <div className="flex items-center justify-between mb-2">
                <label className="text-sm font-medium text-foreground">标题</label>
                <span className="text-xs text-muted-foreground">{title.length}/100</span>
              </div>
              <input
                type="text"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                maxLength={100}
                placeholder="请输入视频标题"
                className="w-full px-3 py-2 bg-background border border-input rounded-lg text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:border-transparent transition-all"
              />
            </div>

            <div>
              <label className="text-sm font-medium text-foreground mb-2 block">描述</label>
              <textarea
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="请输入视频简介/描述"
                rows={4}
                className="w-full px-3 py-2 bg-background border border-input rounded-lg text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:border-transparent transition-all resize-y"
              />
            </div>

            <div>
              <label className="text-sm font-medium text-foreground mb-2 block">标签</label>
              <div className="flex flex-wrap gap-2 mb-2">
                {tags.map((tag) => (
                  <span
                    key={tag}
                    className="inline-flex items-center gap-1 px-2.5 py-1 bg-primary/10 text-primary rounded-full text-xs font-medium"
                  >
                    #{tag}
                    <button onClick={() => removeTag(tag)} className="hover:text-destructive transition-colors">
                      <X className="w-3 h-3" />
                    </button>
                  </span>
                ))}
              </div>
              <input
                type="text"
                value={tagInput}
                onChange={(e) => setTagInput(e.target.value)}
                onKeyDown={handleTagKeyDown}
                placeholder="按回车添加标签"
                className="w-full px-3 py-2 bg-background border border-input rounded-lg text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:border-transparent transition-all"
              />
            </div>
          </div>

          {/* Advanced Options */}
          <div className="bg-card border border-border rounded-xl overflow-hidden">
            <button
              onClick={() => setShowAdvanced(!showAdvanced)}
              className="w-full px-6 py-4 flex items-center justify-between text-sm font-medium text-foreground hover:bg-secondary/50 transition-colors"
            >
              <span>高级选项</span>
              {showAdvanced ? <ChevronUp className="w-4 h-4 text-muted-foreground" /> : <ChevronDown className="w-4 h-4 text-muted-foreground" />}
            </button>
            {showAdvanced && (
              <div className="px-6 pb-4 space-y-3 border-t border-border pt-4">
                <ToggleOption label="声明原创" description="标记为原创作品" checked={isOriginal} onChange={setIsOriginal} />
                <ToggleOption label="定时发布" description="设置指定时间自动发布" checked={isScheduled} onChange={setIsScheduled} />
                <ToggleOption label="手动确认提交" description="填充内容后，需手动点击发布" checked={manualConfirm} onChange={setManualConfirm} />
              </div>
            )}
          </div>
        </div>

        {/* Right Column - Account Selection */}
        <div className="w-[300px] shrink-0 space-y-4">
          <div className="bg-card border border-border rounded-xl p-5">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-sm font-semibold text-foreground">
                选择发布账号{" "}
                <span className="text-muted-foreground font-normal">(已选 {selectedCount})</span>
              </h3>
              <div className="flex gap-2 text-xs">
                <button onClick={() => selectAllAccounts(accounts.map((a) => a.id))} className="text-primary hover:text-primary/80 transition-colors">
                  全选
                </button>
                <span className="text-border">/</span>
                <button onClick={deselectAllAccounts} className="text-primary hover:text-primary/80 transition-colors">
                  全不选
                </button>
              </div>
            </div>

            {accounts.length === 0 ? (
              <div className="text-center py-4">
                <p className="text-sm text-muted-foreground">还没有添加账号</p>
                <a href="/accounts" className="text-xs text-primary hover:text-primary/80 mt-1 inline-block">
                  去添加账号
                </a>
              </div>
            ) : (
              <div className="space-y-2">
                {accounts.map((account) => {
                  const isSelected = selectedAccountIds.includes(account.id);
                  return (
                    <button
                      key={account.id}
                      onClick={() => toggleAccount(account.id)}
                      className={cn(
                        "w-full flex items-center gap-3 p-3 rounded-lg transition-all text-left",
                        isSelected
                          ? "bg-primary/10 border border-primary/30"
                          : "bg-secondary/50 border border-transparent hover:bg-secondary"
                      )}
                    >
                      <div
                        className={cn(
                          "w-5 h-5 rounded-md border-2 flex items-center justify-center shrink-0 transition-colors",
                          isSelected ? "bg-primary border-primary" : "border-input"
                        )}
                      >
                        {isSelected && (
                          <svg className="w-3 h-3 text-primary-foreground" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={3}>
                            <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
                          </svg>
                        )}
                      </div>
                      <PlatformIcon platform={account.platform} size="sm" />
                      <div className="flex-1 min-w-0">
                        <PlatformBadge platform={account.platform} />
                        <p className="text-xs text-muted-foreground truncate mt-0.5">
                          {account.displayName}
                        </p>
                      </div>
                      <div className={cn("w-2 h-2 rounded-full shrink-0", account.isLoggedIn ? "bg-green-500" : "bg-amber-500")} />
                    </button>
                  );
                })}
              </div>
            )}
          </div>

          {/* Publish Button */}
          <button
            onClick={handlePublish}
            disabled={!canPublish}
            className={cn(
              "w-full py-3 rounded-xl text-sm font-semibold transition-all flex items-center justify-center gap-2",
              canPublish
                ? "bg-primary text-primary-foreground hover:opacity-90 shadow-lg shadow-primary/25"
                : "bg-muted text-muted-foreground cursor-not-allowed"
            )}
          >
            {isPublishing ? (
              <>
                <Loader2 className="w-4 h-4 animate-spin" />
                正在触发上传...
              </>
            ) : (
              "开始发布"
            )}
          </button>

          {/* Task Queue */}
          <div className="bg-card border border-border rounded-xl p-5">
            <div className="flex items-center justify-between mb-3">
              <h3 className="text-sm font-semibold text-foreground">任务队列 (0)</h3>
              <button className="text-xs text-destructive hover:text-destructive/80 transition-colors">清空</button>
            </div>
            <div className="text-center py-6">
              <p className="text-sm text-muted-foreground">暂无任务</p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function platformStatusLabel(status: string): string {
  switch (status) {
    case "automated":
      return "自动化已完成";
    case "launched":
      return "已打开 Chrome，需手动继续";
    case "failed":
      return "启动失败";
    default:
      return status;
  }
}

function resolveActionHint(task: PlatformTaskResult): string | null {
  const platformName = PLATFORMS[task.platform as keyof typeof PLATFORMS]?.name ?? "目标平台";
  if (task.error_code === "CDP_NO_PAGE" || task.error_code === "PROFILE_BUSY") {
    return "请先关闭该账号的 Chrome 窗口后重试";
  }
  if (task.error_code === "TARGET_PAGE_NOT_FOUND") {
    return `未定位到${platformName}上传页，已尝试新开窗口。请在 Chrome 打开${platformName}上传页后重试`;
  }
  if (task.error_code === "TARGET_PAGE_NOT_READY") {
    return "页面未完成加载，请等待页面稳定后重试";
  }
  if (task.error_code === "AUTOMATION_TIMEOUT") {
    return "上传可能已开始，请在 Chrome 页面继续并重试提交";
  }
  if (task.session_mode === "reused_existing" && !task.error_code) {
    return "已复用已打开的账号窗口，继续自动上传";
  }
  return task.action_hint ?? null;
}

function automationPhaseText(task: PlatformTaskResult): string | null {
  switch (task.automation_phase) {
    case "upload_started":
      return "已触发上传，后续在 Chrome 完成";
    case "timeout":
      return "自动化超时，已切换手动继续";
    case "manual_continue":
      return "需手动继续";
    case "automation_failed":
      return "自动化失败";
    default:
      return null;
  }
}

function formatSessionMeta(task: PlatformTaskResult): string | null {
  const mode = task.session_mode ?? null;
  const port = task.debug_port_used ?? null;
  const modeLabel = sessionModeLabel(mode);

  if (modeLabel && port != null) {
    return `${modeLabel} / 端口 ${port}`;
  }
  if (modeLabel) {
    return modeLabel;
  }
  if (port != null) {
    return `端口 ${port}`;
  }
  return null;
}

function sessionModeLabel(mode: string | null): string | null {
  switch (mode) {
    case "reused_existing":
      return "复用已有窗口";
    case "launched_new":
      return "新启动窗口";
    case "manual_only":
      return "手动继续";
    default:
      return null;
  }
}

function ToggleOption({ label, description, checked, onChange }: { label: string; description: string; checked: boolean; onChange: (value: boolean) => void }) {
  return (
    <div className="flex items-center justify-between">
      <div>
        <p className="text-sm font-medium text-foreground">{label}</p>
        <p className="text-xs text-muted-foreground">{description}</p>
      </div>
      <button onClick={() => onChange(!checked)} className={cn("relative w-10 h-6 rounded-full transition-colors", checked ? "bg-primary" : "bg-input")}>
        <span className={cn("absolute top-1 w-4 h-4 rounded-full bg-white shadow transition-transform", checked ? "translate-x-5" : "translate-x-1")} />
      </button>
    </div>
  );
}
