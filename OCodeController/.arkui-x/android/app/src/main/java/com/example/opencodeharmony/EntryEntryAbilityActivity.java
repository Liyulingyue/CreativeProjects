package com.example.opencodeharmony;

import android.os.Bundle;
import android.util.Log;
import android.view.WindowManager;
import android.content.res.Configuration;

import ohos.stage.ability.adapter.StageActivity;

import java.lang.reflect.Method;


/**
 * Example ace activity class, which will load ArkUI-X ability instance.
 * StageActivity is provided by ArkUI-X
 * @see <a href=
 * "https://gitee.com/arkui-x/docs/blob/master/zh-cn/application-dev/tutorial/how-to-integrate-arkui-into-android.md">
 * to build android library</a>
 */
public class EntryEntryAbilityActivity extends StageActivity {
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        Log.i("HiHelloWorld", "EntryEntryAbilityActivity");

        getWindow().setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_PAN);

        setInstanceName("com.example.opencodeharmony:entry:EntryAbility:");
        super.onCreate(savedInstanceState);
    }

    @Override
    protected void onResume() {
        super.onResume();
        applyRotationLock();
    }

    @Override
    public void onConfigurationChanged(Configuration newConfig) {
        super.onConfigurationChanged(newConfig);
        applyRotationLock();
    }

    private void applyRotationLock() {
        boolean isLocked = false;
        try {
            Class<?> storageClass = Class.forName("AppStorage");
            Method getInstanceMethod = storageClass.getMethod("getInstance");
            Object storageInstance = getInstanceMethod.invoke(null);

            if (storageInstance != null) {
                Method getBoolMethod = storageClass.getMethod("getBool", String.class, boolean.class);
                isLocked = (Boolean) getBoolMethod.invoke(storageInstance, "rotationLocked", false);
            }
        } catch (Exception e) {
            Log.w("HiHelloWorld", "applyRotationLock: " + e.getMessage());
            try {
                Class<?> stageActivityClass = Class.forName("ohos.stage.ability.adapter.StageActivity");
                Method getAbilityMethod = stageActivityClass.getMethod("getAbility");
                Object ability = getAbilityMethod.invoke(this);
                if (ability != null) {
                    Class<?> abilityClass = Class.forName("ohos.aafwk.ability.Ability");
                    Method getContextMethod = abilityClass.getMethod("getContext");
                    Object context = getContextMethod.invoke(ability);

                    Class<?> dataPreferencesClass = Class.forName("ohos.data.preferences.Preferences");
                    Class<?> dataPreferencesHelperClass = Class.forName("ohos.data.preferences.PreferencesHelper");

                    Method getPreferencesMethod = dataPreferencesHelperClass.getMethod("getPreferences",
                        Class.forName("ohos.aafwk.content.Intent"), String.class);
                    Object preferences = getPreferencesMethod.invoke(null, context.getClass().getMethod("getIntent").invoke(context), "opencode_data");

                    if (preferences != null) {
                        Method getSyncMethod = dataPreferencesClass.getMethod("getSync", String.class, Object.class);
                        Object rotationLocked = getSyncMethod.invoke(preferences, "rotationLocked", "false");
                        isLocked = "true".equals(rotationLocked.toString());
                    }
                }
            } catch (Exception ex) {
                Log.w("HiHelloWorld", "applyRotationLock fallback: " + ex.getMessage());
            }
        }

        Log.i("HiHelloWorld", "applyRotationLock: isLocked=" + isLocked);

        if (isLocked) {
            setRequestedOrientation(android.content.pm.ActivityInfo.SCREEN_ORIENTATION_PORTRAIT);
        } else {
            setRequestedOrientation(android.content.pm.ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED);
        }
    }
}
