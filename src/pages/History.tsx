import { Clock, Search } from "lucide-react";

export function History() {
  return (
    <div className="p-8 max-w-5xl mx-auto">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-foreground">发布历史</h1>
          <p className="text-sm text-muted-foreground mt-1">
            查看你的所有发布记录
          </p>
        </div>
        <div className="flex items-center gap-2">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
            <input
              type="text"
              placeholder="搜索发布记录..."
              className="pl-9 pr-3 py-2 bg-secondary border border-border rounded-lg text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:border-transparent w-[240px]"
            />
          </div>
        </div>
      </div>

      {/* Empty State */}
      <div className="bg-card border border-border rounded-xl p-12 text-center">
        <Clock className="w-12 h-12 text-muted-foreground mx-auto mb-4" />
        <h3 className="text-lg font-semibold text-foreground mb-2">
          暂无发布记录
        </h3>
        <p className="text-sm text-muted-foreground max-w-sm mx-auto">
          发布视频后，你的发布记录将显示在这里，包括各平台的发布状态
        </p>
      </div>
    </div>
  );
}
