import { create } from 'zustand';
import type { Project, AppConfig } from '../types';

interface AppState {
    // 项目
    project: Project | null;
    projectName: string;
    setProject: (p: Project | null) => void;

    // 配置
    appConfig: AppConfig | null;
    setAppConfig: (c: AppConfig) => void;

    // 当前步骤
    currentStep: number;
    setCurrentStep: (n: number) => void;

    // 汇总结果
    aggregationResults: any[];
    setAggregationResults: (r: any[]) => void;

    // 分析结果
    analysisResults: any[];
    setAnalysisResults: (r: any[]) => void;
}

export const useAppStore = create<AppState>((set) => ({
    project: null,
    projectName: '',
    setProject: (p) => set({ project: p, projectName: p?.name ?? '' }),

    appConfig: null,
    setAppConfig: (c) => set({ appConfig: c }),

    currentStep: 0,
    setCurrentStep: (n) => set({ currentStep: n }),

    aggregationResults: [],
    setAggregationResults: (r) => set({ aggregationResults: r }),

    analysisResults: [],
    setAnalysisResults: (r) => set({ analysisResults: r }),
}));
