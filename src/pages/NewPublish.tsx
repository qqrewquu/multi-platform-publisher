import { useState, useCallback, useEffect, type KeyboardEvent, type DragEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-opener";
import {
  Upload,
  X,
  Film,
  ChevronDown,
  ChevronUp,
  ExternalLink,
  CheckCircle,
  AlertCircle,
  Loader2,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { usePublishStore } from "@/stores/publishStore";
import { useAccountStore } from "@/stores/accountStore";
import { PlatformIcon, PlatformBadge } from "@/components/PlatformIcon";
import { PLATFORMS } from "@/types";

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
  const [publishResult, setPublishResult] = useState<any>(null);

  useEffect(() => {
    fetchAccounts();
  }, []);

  const handleDragOver = useCallback((e: DragEvent) => {
    e.preventDefault();
    setIsDragging(true);
  }, []);

  const handleDragLeave = useCallback((e: DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
  }, []);

  const handleDrop = useCallback(
    (e: DragEvent) => {
      e.preventDefault();
      setIsDragging(false);
      const file = e.dataTransfer.files[0];
      if (file && file.type.startsWith("video/")) {
        // In Tauri, we need the full path. For drag-and-drop, we get the file name.
        // The actual file path will be available through Tauri's file system APIs.
        setVideoPath(file.name, file.name);
      }
    },
    [setVideoPath]
  );

  const handleTagKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && tagInput.trim()) {
      e.preventDefault();
      addTag(tagInput.trim());
      setTagInput("");
    }
  };

  const handlePublish = async () => {
    if (selectedAccountIds.length === 0 || !title.trim()) return;

    setIsPublishing(true);
    setPublishResult(null);
    try {
      const result = await invoke("create_publish_task", {
        request: {
          video_path: videoPath || "",
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
      setPublishResult({ error: String(e) });
    } finally {
      setIsPublishing(false);
    }
  };

  const selectedCount = selectedAccountIds.length;

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
            publishResult.error
              ? "bg-destructive/10 border-destructive/20"
              : "bg-green-500/10 border-green-500/20"
          )}
        >
          {publishResult.error ? (
            <AlertCircle className="w-5 h-5 text-destructive shrink-0 mt-0.5" />
          ) : (
            <CheckCircle className="w-5 h-5 text-green-600 dark:text-green-400 shrink-0 mt-0.5" />
          )}
          <div>
            {publishResult.error ? (
              <p className="text-sm text-destructive">{publishResult.error}</p>
            ) : (
              <>
                <p className="text-sm font-medium text-green-600 dark:text-green-400">
                  Chrome 已启动！
                </p>
                <p className="text-xs text-muted-foreground mt-1">
                  请在 Chrome 浏览器中完成视频上传和发布操作
                </p>
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
                  <p className="text-xs text-muted-foreground">视频已选择</p>
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
                onDragOver={handleDragOver}
                onDragLeave={handleDragLeave}
                onDrop={handleDrop}
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
            disabled={selectedCount === 0 || !title.trim() || isPublishing}
            className={cn(
              "w-full py-3 rounded-xl text-sm font-semibold transition-all flex items-center justify-center gap-2",
              selectedCount > 0 && title.trim() && !isPublishing
                ? "bg-primary text-primary-foreground hover:opacity-90 shadow-lg shadow-primary/25"
                : "bg-muted text-muted-foreground cursor-not-allowed"
            )}
          >
            {isPublishing ? (
              <>
                <Loader2 className="w-4 h-4 animate-spin" />
                正在启动...
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
