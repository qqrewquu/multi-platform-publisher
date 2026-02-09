import { useState, useCallback, type KeyboardEvent, type DragEvent } from "react";
import {
  Upload,
  X,
  Film,
  ChevronDown,
  ChevronUp,
  ExternalLink,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { usePublishStore } from "@/stores/publishStore";
import { useAccountStore } from "@/stores/accountStore";
import { PlatformIcon, PlatformBadge } from "@/components/PlatformIcon";
import { PLATFORMS } from "@/types";

export function NewPublish() {
  const {
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

  const accounts = useAccountStore((s) => s.accounts);
  const [tagInput, setTagInput] = useState("");
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [isDragging, setIsDragging] = useState(false);

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

  const selectedCount = selectedAccountIds.length;

  return (
    <div className="p-8 max-w-6xl mx-auto">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold text-foreground">新建发布任务</h1>
        <button
          onClick={resetForm}
          className="text-sm text-primary hover:text-primary/80 transition-colors"
        >
          重置表单
        </button>
      </div>

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

          {/* Title */}
          <div className="bg-card border border-border rounded-xl p-6 space-y-4">
            <div>
              <div className="flex items-center justify-between mb-2">
                <label className="text-sm font-medium text-foreground">
                  标题
                </label>
                <span className="text-xs text-muted-foreground">
                  {title.length}/100
                </span>
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

            {/* Description */}
            <div>
              <label className="text-sm font-medium text-foreground mb-2 block">
                描述
              </label>
              <textarea
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="请输入视频简介/描述"
                rows={4}
                className="w-full px-3 py-2 bg-background border border-input rounded-lg text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:border-transparent transition-all resize-y"
              />
            </div>

            {/* Tags */}
            <div>
              <label className="text-sm font-medium text-foreground mb-2 block">
                标签
              </label>
              <div className="flex flex-wrap gap-2 mb-2">
                {tags.map((tag) => (
                  <span
                    key={tag}
                    className="inline-flex items-center gap-1 px-2.5 py-1 bg-primary/10 text-primary rounded-full text-xs font-medium"
                  >
                    #{tag}
                    <button
                      onClick={() => removeTag(tag)}
                      className="hover:text-destructive transition-colors"
                    >
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
              {showAdvanced ? (
                <ChevronUp className="w-4 h-4 text-muted-foreground" />
              ) : (
                <ChevronDown className="w-4 h-4 text-muted-foreground" />
              )}
            </button>
            {showAdvanced && (
              <div className="px-6 pb-4 space-y-3 border-t border-border pt-4">
                <ToggleOption
                  label="声明原创"
                  description="标记为原创作品"
                  checked={isOriginal}
                  onChange={setIsOriginal}
                />
                <ToggleOption
                  label="定时发布"
                  description="设置指定时间自动发布"
                  checked={isScheduled}
                  onChange={setIsScheduled}
                />
                <ToggleOption
                  label="手动确认提交"
                  description="填充内容后，需手动点击发布"
                  checked={manualConfirm}
                  onChange={setManualConfirm}
                />
              </div>
            )}
          </div>
        </div>

        {/* Right Column - Account Selection */}
        <div className="w-[300px] shrink-0 space-y-4">
          {/* Account Selection */}
          <div className="bg-card border border-border rounded-xl p-5">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-sm font-semibold text-foreground">
                选择发布账号{" "}
                <span className="text-muted-foreground font-normal">
                  (已选 {selectedCount})
                </span>
              </h3>
              <div className="flex gap-2 text-xs">
                <button
                  onClick={() =>
                    selectAllAccounts(accounts.map((a) => a.id))
                  }
                  className="text-primary hover:text-primary/80 transition-colors"
                >
                  全选
                </button>
                <span className="text-border">/</span>
                <button
                  onClick={deselectAllAccounts}
                  className="text-primary hover:text-primary/80 transition-colors"
                >
                  全不选
                </button>
              </div>
            </div>

            <div className="space-y-2">
              {accounts.map((account) => {
                const isSelected = selectedAccountIds.includes(account.id);
                const info = PLATFORMS[account.platform];
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
                    {/* Checkbox */}
                    <div
                      className={cn(
                        "w-5 h-5 rounded-md border-2 flex items-center justify-center shrink-0 transition-colors",
                        isSelected
                          ? "bg-primary border-primary"
                          : "border-input"
                      )}
                    >
                      {isSelected && (
                        <svg
                          className="w-3 h-3 text-primary-foreground"
                          fill="none"
                          viewBox="0 0 24 24"
                          stroke="currentColor"
                          strokeWidth={3}
                        >
                          <path
                            strokeLinecap="round"
                            strokeLinejoin="round"
                            d="M5 13l4 4L19 7"
                          />
                        </svg>
                      )}
                    </div>

                    <PlatformIcon platform={account.platform} size="sm" />

                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <PlatformBadge platform={account.platform} />
                      </div>
                      <p className="text-xs text-muted-foreground truncate mt-0.5">
                        {account.displayName}
                      </p>
                    </div>

                    {/* Status dot */}
                    <div
                      className={cn(
                        "w-2 h-2 rounded-full shrink-0",
                        account.isLoggedIn ? "bg-green-500" : "bg-amber-500"
                      )}
                    />

                    <a
                      href={info.creatorUrl}
                      target="_blank"
                      rel="noopener noreferrer"
                      onClick={(e) => e.stopPropagation()}
                      className="text-muted-foreground hover:text-foreground transition-colors"
                    >
                      <ExternalLink className="w-3.5 h-3.5" />
                    </a>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Publish Button */}
          <button
            className={cn(
              "w-full py-3 rounded-xl text-sm font-semibold transition-all",
              selectedCount > 0 && title.trim()
                ? "bg-primary text-primary-foreground hover:opacity-90 shadow-lg shadow-primary/25"
                : "bg-muted text-muted-foreground cursor-not-allowed"
            )}
            disabled={selectedCount === 0 || !title.trim()}
          >
            开始发布
          </button>

          {/* Task Queue */}
          <div className="bg-card border border-border rounded-xl p-5">
            <div className="flex items-center justify-between mb-3">
              <h3 className="text-sm font-semibold text-foreground">
                任务队列 (0)
              </h3>
              <button className="text-xs text-destructive hover:text-destructive/80 transition-colors">
                清空
              </button>
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

// Toggle Switch Component
function ToggleOption({
  label,
  description,
  checked,
  onChange,
}: {
  label: string;
  description: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between">
      <div>
        <p className="text-sm font-medium text-foreground">{label}</p>
        <p className="text-xs text-muted-foreground">{description}</p>
      </div>
      <button
        onClick={() => onChange(!checked)}
        className={cn(
          "relative w-10 h-6 rounded-full transition-colors",
          checked ? "bg-primary" : "bg-input"
        )}
      >
        <span
          className={cn(
            "absolute top-1 w-4 h-4 rounded-full bg-white shadow transition-transform",
            checked ? "translate-x-5" : "translate-x-1"
          )}
        />
      </button>
    </div>
  );
}
