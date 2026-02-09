import { useEffect } from "react";
import { Upload, Users, Clock, FileText, ArrowRight } from "lucide-react";
import { Link } from "react-router-dom";
import { useAccountStore } from "@/stores/accountStore";
import { cn } from "@/lib/utils";

export function Dashboard() {
  const { accounts, fetchAccounts } = useAccountStore();

  useEffect(() => {
    fetchAccounts();
  }, []);

  const loggedInCount = accounts.filter((a) => a.isLoggedIn).length;

  const stats = [
    {
      label: "已连接账号",
      value: `${loggedInCount}/${accounts.length}`,
      icon: Users,
      color: "text-green-500",
      bgColor: "bg-green-500/10",
    },
    {
      label: "今日发布",
      value: "0",
      icon: Upload,
      color: "text-primary",
      bgColor: "bg-primary/10",
    },
    {
      label: "待发布任务",
      value: "0",
      icon: Clock,
      color: "text-amber-500",
      bgColor: "bg-amber-500/10",
    },
    {
      label: "模板数量",
      value: "0",
      icon: FileText,
      color: "text-purple-500",
      bgColor: "bg-purple-500/10",
    },
  ];

  const quickActions = [
    {
      label: "发布新视频",
      description: "上传视频并发布到多个平台",
      to: "/publish",
      icon: Upload,
      primary: true,
    },
    {
      label: "管理账号",
      description: "添加或检查平台账号状态",
      to: "/accounts",
      icon: Users,
      primary: false,
    },
    {
      label: "创建模板",
      description: "保存常用的标题和描述模板",
      to: "/templates",
      icon: FileText,
      primary: false,
    },
  ];

  return (
    <div className="p-8 max-w-5xl mx-auto">
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-foreground">仪表盘</h1>
        <p className="text-muted-foreground mt-1">欢迎回来！快速查看你的发布状态。</p>
      </div>

      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-8">
        {stats.map((stat) => (
          <div key={stat.label} className="bg-card border border-border rounded-xl p-4 flex items-center gap-3">
            <div className={cn("w-10 h-10 rounded-lg flex items-center justify-center", stat.bgColor)}>
              <stat.icon className={cn("w-5 h-5", stat.color)} />
            </div>
            <div>
              <p className="text-2xl font-bold text-foreground">{stat.value}</p>
              <p className="text-xs text-muted-foreground">{stat.label}</p>
            </div>
          </div>
        ))}
      </div>

      <div className="mb-8">
        <h2 className="text-lg font-semibold text-foreground mb-4">快速操作</h2>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          {quickActions.map((action) => (
            <Link
              key={action.to}
              to={action.to}
              className={cn(
                "group border rounded-xl p-5 transition-all duration-200 hover:shadow-lg flex flex-col gap-3",
                action.primary
                  ? "bg-primary text-primary-foreground border-primary hover:opacity-90"
                  : "bg-card border-border hover:border-primary/50"
              )}
            >
              <div className="flex items-center justify-between">
                <action.icon className={cn("w-6 h-6", action.primary ? "text-primary-foreground" : "text-primary")} />
                <ArrowRight className={cn("w-4 h-4 opacity-0 group-hover:opacity-100 transition-opacity", action.primary ? "text-primary-foreground" : "text-muted-foreground")} />
              </div>
              <div>
                <h3 className={cn("font-semibold", action.primary ? "text-primary-foreground" : "text-foreground")}>{action.label}</h3>
                <p className={cn("text-sm mt-0.5", action.primary ? "text-primary-foreground/80" : "text-muted-foreground")}>{action.description}</p>
              </div>
            </Link>
          ))}
        </div>
      </div>

      <div>
        <h2 className="text-lg font-semibold text-foreground mb-4">最近活动</h2>
        <div className="bg-card border border-border rounded-xl p-8 text-center">
          <Clock className="w-10 h-10 text-muted-foreground mx-auto mb-3" />
          <p className="text-muted-foreground">暂无发布记录</p>
          <p className="text-sm text-muted-foreground mt-1">发布视频后，你的活动记录将显示在这里</p>
        </div>
      </div>
    </div>
  );
}
