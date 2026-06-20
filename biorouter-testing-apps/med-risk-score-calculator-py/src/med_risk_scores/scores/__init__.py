"""
Clinical risk score modules.

Importing this package registers all built-in scores with the global registry.
"""
from med_risk_scores.scores.cha2ds2_vasc import cha2ds2_vasc as _  # noqa: F401
from med_risk_scores.scores.has_bled import has_bled as _  # noqa: F401
from med_risk_scores.scores.wells import wells_dvt as _  # noqa: F401
from med_risk_scores.scores.wells import wells_pe as _  # noqa: F401
from med_risk_scores.scores.curb65 import curb65 as _  # noqa: F401
from med_risk_scores.scores.meld import meld as _  # noqa: F401
from med_risk_scores.scores.meld import meld_na as _  # noqa: F401
from med_risk_scores.scores.qsofa import qsofa as _  # noqa: F401
from med_risk_scores.scores.framingham import framingham_risk_score as _  # noqa: F401
from med_risk_scores.scores.framingham import ascvd_10yr as _  # noqa: F401
from med_risk_scores.scores.apache_ii import apache_ii_lite as _  # noqa: F401
