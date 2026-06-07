import baseStyles from "./styles/pageBase.module.css";
import recordsStyles from "./styles/pageRecords.module.css";
import statsAndSettingsStyles from "./styles/pageStatsAndSettings.module.css";
import statsDetailsAndRandomizerStyles from "./styles/pageStatsDetailsAndRandomizer.module.css";
import responsiveThemeStyles from "./styles/pageResponsiveTheme.module.css";

type CssModule = Readonly<Record<string, string>>;

class ConfigStyles {
    private readonly styles: CssModule;

    public constructor(modules: readonly CssModule[]) {
        const merged: Record<string, string> = {};
        for (const moduleStyles of modules) {
            for (const [key, value] of Object.entries(moduleStyles)) {
                merged[key] = merged[key] ? `${merged[key]} ${value}` : value;
            }
        }
        this.styles = merged;
    }

    public value(): CssModule {
        return this.styles;
    }
}

const styles = new ConfigStyles([
    baseStyles,
    recordsStyles,
    statsAndSettingsStyles,
    statsDetailsAndRandomizerStyles,
    responsiveThemeStyles,
]).value();

export default styles;
