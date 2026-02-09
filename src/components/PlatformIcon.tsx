import type { PlatformType } from "@/types";
import { PLATFORMS } from "@/types";
import { cn } from "@/lib/utils";

interface PlatformIconProps {
  platform: PlatformType;
  size?: "sm" | "md" | "lg";
  className?: string;
}

export function PlatformIcon({ platform, size = "md", className }: PlatformIconProps) {
  const info = PLATFORMS[platform];
  const sizeClasses = {
    sm: "w-6 h-6 text-[10px]",
    md: "w-8 h-8 text-xs",
    lg: "w-10 h-10 text-sm",
  };

  // Short labels for each platform
  const labels: Record<PlatformType, string> = {
    douyin: "抖",
    xiaohongshu: "小红",
    bilibili: "B",
    wechat: "微",
    youtube: "YT",
  };

  return (
    <div
      className={cn(
        "rounded-lg flex items-center justify-center font-bold text-white shrink-0",
        sizeClasses[size],
        className
      )}
      style={{ backgroundColor: info.bgColor }}
    >
      {labels[platform]}
    </div>
  );
}

interface PlatformBadgeProps {
  platform: PlatformType;
  className?: string;
}

export function PlatformBadge({ platform, className }: PlatformBadgeProps) {
  const info = PLATFORMS[platform];
  return (
    <span
      className={cn(
        "inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium text-white",
        className
      )}
      style={{ backgroundColor: info.bgColor }}
    >
      {info.name}
    </span>
  );
}
