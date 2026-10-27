
@{
    RootModule        = 'DevShellTools.psm1'
    ModuleVersion     = '1.0.4'
    GUID              = 'b7dc2664-8f08-42ba-a37b-a38886d589fd'
    Author            = 'OpenAI'
    CompanyName       = 'User-local'
    Copyright         = '(c) 2026'
    Description       = '可扩展的 Windows PowerShell 开发与运维快捷命令模块。'
    PowerShellVersion = '5.1'
    FunctionsToExport = @(
        'dsh',
        'lt','ltf','ltd','ll','la','mkcd','up','up2','open','here','sz',
        'super','isadmin','psb','reload','profile','which',
        'lpr',
        'gs','gb','gco','gsw','gswc','ga','gaa','gcmsg','gpp','gpl','gf','gg','gd','gds','grv','gclean',
        'ports','port','myip','dns','pingx','curlh','nettest','killport'
    )
    CmdletsToExport   = @()
    VariablesToExport = @()
    AliasesToExport   = @()
    PrivateData = @{
        PSData = @{
            Tags = @('PowerShell','DevOps','Git','Proxy','Network')
            ProjectUri = 'https://repo.mvp.edu.kg/QAS/WinShellTools'
        }
    }
}
