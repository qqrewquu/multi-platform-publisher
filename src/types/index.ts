// Platform types
export type PlatformType = "douyin" | "xiaohongshu" | "bilibili" | "wechat" | "youtube";

export interface PlatformInfo {
  id: PlatformType;
  name: string;
  nameEn: string;
  color: string;
  bgColor: string;
  creatorUrl: string;
  icon: string;
}

export const PLATFORMS: Record<PlatformType, PlatformInfo> = {
  douyin: {
    id: "douyin",
    name: "抖音",
    nameEn: "Douyin",
    color: "#000000",
    bgColor: "#fe2c55",
    creatorUrl: "https://creator.douyin.com",
    icon: "douyin",
  },
  xiaohongshu: {
    id: "xiaohongshu",
    name: "小红书",
    nameEn: "Xiaohongshu",
    color: "#ff2442",
    bgColor: "#ff2442",
    creatorUrl: "https://creator.xiaohongshu.com",
    icon: "xiaohongshu",
  },
  bilibili: {
    id: "bilibili",
    name: "哔哩哔哩",
    nameEn: "Bilibili",
    color: "#00a1d6",
    bgColor: "#fb7299",
    creatorUrl: "https://member.bilibili.com",
    icon: "bilibili",
  },
  wechat: {
    id: "wechat",
    name: "微信视频号",
    nameEn: "WeChat",
    color: "#07c160",
    bgColor: "#07c160",
    creatorUrl: "https://channels.weixin.qq.com",
    icon: "wechat",
  },
  youtube: {
    id: "youtube",
    name: "YouTube",
    nameEn: "YouTube",
    color: "#ff0000",
    bgColor: "#ff0000",
    creatorUrl: "https://studio.youtube.com",
    icon: "youtube",
  },
};

// Account types
export interface Account {
  id: number;
  platform: PlatformType;
  displayName: string;
  avatarUrl?: string;
  chromeProfileDir: string;
  isLoggedIn: boolean;
  lastCheckedAt?: string;
  createdAt: string;
}

// Publish task types
export type TaskStatus = "pending" | "publishing" | "completed" | "partial" | "failed";
export type PlatformTaskStatus = "pending" | "uploading" | "filling" | "waiting_confirm" | "published" | "failed";

export interface PublishTask {
  id: number;
  videoPath: string;
  title: string;
  description?: string;
  tags: string[];
  coverPath?: string;
  isOriginal: boolean;
  status: TaskStatus;
  scheduledAt?: string;
  createdAt: string;
  platforms: PublishTaskPlatform[];
}

export interface PublishTaskPlatform {
  id: number;
  taskId: number;
  accountId: number;
  customTitle?: string;
  customDescription?: string;
  customTags?: string[];
  status: PlatformTaskStatus;
  errorMessage?: string;
  publishedAt?: string;
}

// Template types
export interface Template {
  id: number;
  name: string;
  titleTemplate?: string;
  descriptionTemplate?: string;
  tags: string[];
  createdAt: string;
  updatedAt: string;
}
